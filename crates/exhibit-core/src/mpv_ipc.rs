//! The small part of mpv's JSON IPC that Exhibit needs: connect to mpv's named pipe,
//! send a command, read JSON lines until the matching reply.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::error::{Error, Result};

/// Open mpv's IPC pipe (`\.\pipe\name`), retrying until it exists or `timeout` passes.
#[cfg(windows)]
pub fn connect(pipe: &str, timeout: std::time::Duration) -> Result<std::fs::File> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(pipe)
        {
            Ok(f) => return Ok(f),
            Err(e) if std::time::Instant::now() >= deadline => {
                return Err(Error::Tool(format!("cannot connect to mpv at {pipe}: {e}")));
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
}

#[cfg(not(windows))]
pub fn connect(_pipe: &str, _timeout: std::time::Duration) -> Result<std::fs::File> {
    Err(Error::Tool(
        "mpv probing is not supported on this platform yet".into(),
    ))
}

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Ask mpv for a property. Event lines and replies to other requests are skipped.
pub fn get_property(stream: &mut (impl Read + Write), name: &str) -> Result<Value> {
    let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let cmd = json!({ "command": ["get_property", name], "request_id": id });
    stream.write_all(format!("{cmd}\n").as_bytes())?;
    stream.flush()?;
    loop {
        let line = read_line(stream)?;
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if msg.get("request_id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        return match msg.get("error").and_then(Value::as_str) {
            Some("success") => Ok(msg.get("data").cloned().unwrap_or(Value::Null)),
            other => Err(Error::Tool(format!(
                "mpv: cannot read {name}: {}",
                other.unwrap_or("no error field")
            ))),
        };
    }
}

/// `get_property`, retrying while mpv says the property is unavailable (it appears only
/// once the file is loaded or the first frame decoded) until `deadline`.
pub fn get_when_ready(
    stream: &mut (impl Read + Write),
    name: &str,
    deadline: Instant,
) -> Result<Value> {
    loop {
        match get_property(stream, name) {
            Err(Error::Tool(m)) if m.contains("unavailable") && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            other => return other,
        }
    }
}

/// One line, read a byte at a time so nothing past it is consumed.
fn read_line(stream: &mut impl Read) -> Result<String> {
    let mut line = Vec::new();
    let mut b = [0u8; 1];
    loop {
        if stream.read(&mut b)? == 0 {
            return Err(Error::Tool("mpv closed the connection".into()));
        }
        if b[0] == b'\n' {
            return Ok(String::from_utf8_lossy(&line).into_owned());
        }
        line.push(b[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Answers each written command with an event, a reply to another request, then the
    /// real reply built from `reply`.
    struct Fake {
        input: VecDeque<u8>,
        reply: &'static str,
    }

    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = buf.len().min(self.input.len());
            for slot in &mut buf[..n] {
                *slot = self.input.pop_front().unwrap();
            }
            Ok(n)
        }
    }

    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let req: Value = serde_json::from_slice(buf).unwrap();
            let id = req["request_id"].as_u64().unwrap();
            let lines = format!(
                "{{\"event\":\"property-change\",\"name\":\"x\"}}\n\
                 {{\"request_id\":{},\"error\":\"success\",\"data\":\"stale\"}}\n\
                 not json\n\
                 {{\"request_id\":{id},{}}}\n",
                id + 1000,
                self.reply
            );
            self.input.extend(lines.bytes());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn fake(reply: &'static str) -> Fake {
        Fake {
            input: VecDeque::new(),
            reply,
        }
    }

    #[test]
    fn matches_request_id_and_skips_events() {
        let mut s = fake(r#""error":"success","data":[1,2]"#);
        assert_eq!(get_property(&mut s, "track-list").unwrap(), json!([1, 2]));
        // A second call on the same stream still works.
        assert_eq!(get_property(&mut s, "duration").unwrap(), json!([1, 2]));
    }

    #[test]
    fn error_reply_is_an_error() {
        let mut s = fake(r#""error":"property unavailable""#);
        let e = get_property(&mut s, "video-params").unwrap_err();
        assert!(e.to_string().contains("property unavailable"), "{e}");
    }

    /// Replies "property unavailable" `n` times, then `data` 42.
    struct Late {
        input: VecDeque<u8>,
        unavailable_left: u32,
    }

    impl Read for Late {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = buf.len().min(self.input.len());
            for slot in &mut buf[..n] {
                *slot = self.input.pop_front().unwrap();
            }
            Ok(n)
        }
    }

    impl Write for Late {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let req: Value = serde_json::from_slice(buf).unwrap();
            let id = req["request_id"].as_u64().unwrap();
            let reply = if self.unavailable_left > 0 {
                self.unavailable_left -= 1;
                format!("{{\"request_id\":{id},\"error\":\"property unavailable\"}}\n")
            } else {
                format!("{{\"request_id\":{id},\"error\":\"success\",\"data\":42}}\n")
            };
            self.input.extend(reply.bytes());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn late(n: u32) -> Late {
        Late {
            input: VecDeque::new(),
            unavailable_left: n,
        }
    }

    #[test]
    fn get_when_ready_retries_until_available() {
        let mut s = late(2);
        let deadline = Instant::now() + Duration::from_secs(10);
        assert_eq!(
            get_when_ready(&mut s, "video-params", deadline).unwrap(),
            42
        );
    }

    #[test]
    fn get_when_ready_gives_up_at_the_deadline() {
        let mut s = late(u32::MAX);
        let deadline = Instant::now() + Duration::from_millis(200);
        let e = get_when_ready(&mut s, "video-params", deadline).unwrap_err();
        assert!(e.to_string().contains("unavailable"), "{e}");
    }

    #[test]
    fn closed_stream_is_an_error() {
        struct Eof;
        impl Read for Eof {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Ok(0)
            }
        }
        impl Write for Eof {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(get_property(&mut Eof, "duration").is_err());
    }
}
