//! Drives the built `exhibit` binary against a temporary data directory and media folder.
//! Never writes a secret: the credential service name is a throwaway.

use serde_json::Value;
use std::path::Path;
use std::process::Command;

struct Cli<'a> {
    data: &'a Path,
}

impl Cli<'_> {
    /// Runs the binary; returns (exit code, parsed stdout).
    fn run(&self, args: &[&str]) -> (i32, Value) {
        let out = Command::new(env!("CARGO_BIN_EXE_exhibit"))
            .args(args)
            .env("EXHIBIT_DATA_DIR", self.data)
            .env(
                "EXHIBIT_KEYRING_SERVICE",
                format!("exhibit-test-{}", std::process::id()),
            )
            .env("PATH", "")
            .output()
            .expect("run exhibit");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let json = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("stdout of {args:?} is not JSON ({e}): {stdout}"));
        (out.status.code().unwrap_or(-1), json)
    }

    fn ok(&self, args: &[&str]) -> Value {
        let (code, json) = self.run(args);
        assert_eq!(code, 0, "{args:?} failed: {json}");
        json
    }
}

#[test]
fn library_flow_through_the_binary() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    for name in [
        "The.Matrix.1999.1080p.mkv",
        "Heat.1995.mkv",
        "Paprika.2006.mkv",
    ] {
        std::fs::write(media.path().join(name), b"").unwrap();
    }
    let cli = Cli { data: data.path() };

    let source = cli.ok(&["source", "add", media.path().to_str().unwrap()]);
    assert!(source["id"].is_i64(), "{source}");
    assert_eq!(cli.ok(&["source", "list"]).as_array().unwrap().len(), 1);

    let scan = cli.ok(&["scan", "--no-probe", "--no-identify"]);
    assert_eq!(scan["scanned"][0]["added"], 3, "{scan}");
    assert!(scan["probed"].is_null() && scan["identified"].is_null());

    let all = cli.ok(&["search"]);
    let all = all.as_array().unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|i| i["kind"] == "other"));
    assert_eq!(cli.ok(&["search", "matrix"]).as_array().unwrap().len(), 1);

    let id = all[0]["id"].as_i64().unwrap().to_string();
    assert_eq!(cli.ok(&["loved", &id]), serde_json::json!({"ok": true}));
    let loved = cli.ok(&["shelf", "loved"]);
    assert_eq!(loved.as_array().unwrap().len(), 1);
    assert_eq!(loved[0]["id"].to_string(), id);

    let (code, err) = cli.run(&["item", "99999"]);
    assert_eq!(code, 1);
    assert_eq!(err["error"]["kind"], "not_found");

    let (code, err) = cli.run(&["identify"]);
    assert_eq!(code, 1);
    assert!(
        err["error"]["message"]
            .as_str()
            .unwrap()
            .contains("TMDB key")
    );

    let status = cli.ok(&["status"]);
    assert_eq!(status["other"], 3);
    assert_eq!(status["tools"].as_array().unwrap().len(), 3);
    assert_eq!(status["tmdb_key"], false);
}

#[test]
fn scan_without_key_or_tools_skips_identification_quietly() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    std::fs::write(media.path().join("Heat.1995.mkv"), b"").unwrap();
    let cli = Cli { data: data.path() };
    cli.ok(&["source", "add", media.path().to_str().unwrap()]);
    let scan = cli.ok(&["scan"]);
    assert_eq!(scan["scanned"][0]["added"], 1);
    // Tools may exist on the dev machine (extra search dirs), so only identification is certain.
    assert!(scan["identified"].is_null(), "{scan}");
}
