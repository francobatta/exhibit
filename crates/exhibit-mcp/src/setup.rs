//! Setting an agent up, as a hosted service does with a setup link: the owner hands their agent
//! one line, which sends it to an instructions file; the file says where this server is, how to
//! register it with the agent's own client for good, and how to check it works. Both start from
//! the exe that asks, so they always name the program on this PC and the library the window shows.

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// What points a run at a library other than the owner's own: carried into the server's
/// registration when set, so an agent reads the same library as the window that wrote the message.
const LIBRARY_ENV: [&str; 3] = [
    "EXHIBIT_DATA_DIR",
    "EXHIBIT_KEYRING_SERVICE",
    "EXHIBIT_TMDB_BUILTIN",
];

/// How a client starts this server: the exe with the argument `mcp`, and the environment that
/// differs from an ordinary run (none for an installed Exhibit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub exe: PathBuf,
    pub env: Vec<(String, String)>,
}

impl Launch {
    /// This process: its own exe and the library variables set for it, with a relative data
    /// directory made absolute, since a client starts the server from a folder of its own.
    pub fn current() -> std::io::Result<Launch> {
        let exe = plain_path(std::env::current_exe()?);
        let mut env = Vec::new();
        for name in LIBRARY_ENV {
            if let Some(value) = std::env::var_os(name) {
                let mut value = PathBuf::from(value);
                if name == "EXHIBIT_DATA_DIR" && value.is_relative() {
                    value = std::env::current_dir()?.join(value).components().collect();
                }
                env.push((name.to_string(), value.to_string_lossy().into_owned()));
            }
        }
        Ok(Launch { exe, env })
    }
}

/// Without Windows' `\\?\` prefix, which clients and shells read badly.
fn plain_path(p: PathBuf) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => p,
    }
}

/// A shell's quoting of a path: in double quotes.
fn quoted(p: &Path) -> String {
    format!("\"{}\"", p.display())
}

/// The file the instructions are written to, in the library's data directory.
pub const INSTRUCTIONS_FILE: &str = "agent-setup.md";

/// The one line the owner copies to their AI agent, like a hosted service's setup link: it sends
/// the agent to the instructions file, which says the rest. `reply_in` names the language the
/// agent should answer in, when it is not English.
pub fn prompt(instructions: &Path, reply_in: Option<&str>) -> String {
    let mut p = format!(
        "Read and follow the instructions in {} to set me up for Exhibit.",
        quoted(instructions)
    );
    if let Some(lang) = reply_in {
        let _ = write!(p, " Talk to me in {lang}.");
    }
    p
}

/// Writes the instructions for `launch` into `dir` (the library's data directory), replacing any
/// written before, so they always name this exe and this library. Answers the file's full path,
/// which the agent reads from a folder of its own.
pub fn write_instructions(launch: &Launch, dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let dir: PathBuf = std::path::absolute(dir)?.components().collect();
    let path = plain_path(dir.join(INSTRUCTIONS_FILE));
    std::fs::write(&path, instructions(launch))?;
    Ok(path)
}

/// What an agent needs to connect itself to this server for good, as Markdown: where the server
/// is, how to check it, how to register it for every future session in whichever client it is,
/// and how to confirm it connects.
pub fn instructions(launch: &Launch) -> String {
    let exe = quoted(&launch.exe);
    let mut p = String::new();
    let _ = writeln!(
        p,
        "# Set the user up for Exhibit\n\n\
         Exhibit is a home-theatre gallery on this PC: the user's own films and shows, played in mpv. \
         It has an MCP server. Connect yourself to it, permanently, so that in any future session you can see the user's library, \
         suggest what to watch, and hang exhibitions (curated shelves) on their Home screen.\n\n\
         ## The server\n\n\
         - Transport: stdio\n\
         - Command: `{exe}`\n\
         - Arguments: `mcp`"
    );
    if !launch.env.is_empty() {
        let _ = writeln!(
            p,
            "- Environment (it points the server at the library the user uses; keep it):"
        );
        for (k, v) in &launch.env {
            let _ = writeln!(p, "  - `{k}={v}`");
        }
    }
    let _ = writeln!(
        p,
        "\nIt needs no key, account, port or extra install, and Exhibit's window does not have to be open.\n\n\
         ## Steps\n\n\
         1. **Check that it works.** Run `{exe} mcp --check`. It starts the server as a client would, talks to it, and prints what the library holds or what is wrong. \
         Exhibit is a windowed app, so from PowerShell run it through cmd, as `cmd /c '{exe} mcp --check'`; otherwise PowerShell returns before it answers.\n\n\
         2. **Register it with your own MCP client, for good**, under the name `exhibit`, at user level: for every session and every folder, not only this project or this conversation. \
         If an `exhibit` server is already registered, replace it. For example:"
    );
    let env_flags = |flag: &str| -> String {
        launch
            .env
            .iter()
            .map(|(k, v)| format!(" {flag} {k}=\"{v}\""))
            .collect()
    };
    let _ = writeln!(
        p,
        "   - Claude Code: `claude mcp add exhibit --scope user{} -- {exe} mcp`. If it says exhibit already exists, run `claude mcp remove exhibit --scope user` first.",
        env_flags("-e")
    );
    let mut desktop = json!({ "command": launch.exe.display().to_string(), "args": ["mcp"] });
    if !launch.env.is_empty() {
        desktop["env"] = Value::Object(
            launch
                .env
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect(),
        );
    }
    let _ = writeln!(
        p,
        "   - Claude Desktop: in `claude_desktop_config.json` (Settings > Developer > Edit Config; usually `%APPDATA%\\Claude\\claude_desktop_config.json`), add this under `mcpServers`, keeping what is there: `\"exhibit\": {desktop}`. \
         Then the user must quit Claude Desktop fully (right-click its tray icon > Quit) and open it again."
    );
    let _ = writeln!(
        p,
        "   - Codex CLI: `codex mcp add exhibit{} -- {exe} mcp`.\n   \
         - Any other client: its user-level MCP settings, as a stdio server with the command, arguments{} above.",
        env_flags("--env"),
        if launch.env.is_empty() {
            ""
        } else {
            " and environment"
        }
    );
    let _ = writeln!(
        p,
        "\n3. **Confirm that your client connects to it**, if it can tell you: in Claude Code, `claude mcp list` shows exhibit as Connected. \
         A client usually loads new servers only in a new session, so Exhibit's tools may not appear in this one; that is expected.\n\n\
         4. **Tell the user**, in two or three short sentences, what you set up, and that from their next session they can simply ask things like \
         \"What should we watch tonight?\" or \"Hang an exhibition of 1970s thrillers on my Home screen.\"\n\n\
         Change nothing else in your settings. Exhibit's tools never touch the user's media files; they write only Exhibit's own state."
    );
    p
}

/// What `--check` found: the server's tools and prompts, and what the library holds.
#[derive(Debug)]
pub struct Checked {
    pub tools: usize,
    pub prompts: usize,
    pub counts: Value,
}

/// Starts `launch` as a client would, speaks MCP to it, and asks it for the library overview.
/// An error says what went wrong in words the owner's agent can act on.
pub fn check(launch: &Launch) -> Result<Checked, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(30), check_async(launch))
            .await
            .unwrap_or_else(|_| Err("the server did not answer within 30 seconds".to_string()))
    })
}

async fn check_async(launch: &Launch) -> Result<Checked, String> {
    let mut cmd = tokio::process::Command::new(&launch.exe);
    cmd.arg("mcp").envs(launch.env.iter().map(|(k, v)| (k, v)));
    let (transport, stderr) = TokioChildProcess::builder(cmd)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", launch.exe.display()))?;
    // A server that fails to start says why on stderr, then closes.
    let why = |stderr: Option<tokio::process::ChildStderr>| async move {
        let mut text = String::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_string(&mut text).await;
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            "the server closed without saying why".to_string()
        } else {
            text
        }
    };
    let client = match ().serve(transport).await {
        Ok(c) => c,
        Err(e) => return Err(format!("{} ({e})", why(stderr).await)),
    };
    let result = async {
        let tools = client.list_all_tools().await.map_err(|e| e.to_string())?;
        let prompts = client.list_all_prompts().await.map_err(|e| e.to_string())?;
        let overview = client
            .call_tool(CallToolRequestParams::new("library_overview"))
            .await
            .map_err(|e| e.to_string())?;
        let text = overview
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default();
        if overview.is_error == Some(true) {
            return Err(format!("the library could not be read: {text}"));
        }
        let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok(Checked {
            tools: tools.len(),
            prompts: prompts.len(),
            counts: v.get("counts").cloned().unwrap_or(Value::Null),
        })
    }
    .await;
    let _ = client.cancel().await;
    result
}

/// The check's report, for a person or an agent to read.
pub fn report(launch: &Launch, checked: &Result<Checked, String>) -> String {
    let start = format!("{} mcp", quoted(&launch.exe));
    match checked {
        Ok(c) => {
            let n = |k: &str| c.counts.get(k).and_then(Value::as_u64).unwrap_or(0);
            let (movies, shows, other, review) =
                (n("movies"), n("shows"), n("other"), n("needs_review"));
            let library = if movies + shows + other == 0 {
                "empty so far. Add a folder in Exhibit (Settings > Folders) and it fills in."
                    .to_string()
            } else {
                let mut s = format!(
                    "{movies} {}, {shows} {}, {other} other",
                    plural(movies, "film", "films"),
                    plural(shows, "show", "shows")
                );
                if review > 0 {
                    let _ = write!(s, ", {review} in needs review");
                }
                s
            };
            format!(
                "Exhibit's MCP server works.\n  Server:  exhibit {}, {} tools, {} prompts\n  Library: {library}\n  Start it with: {start}",
                env!("CARGO_PKG_VERSION"),
                c.tools,
                c.prompts
            )
        }
        Err(e) => format!(
            "Exhibit's MCP server did not work: {e}\n  Started as: {start}\n  If Exhibit was just installed, open it once, then try again."
        ),
    }
}

fn plural<'a>(n: u64, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> Launch {
        Launch {
            exe: PathBuf::from(r"C:\Users\Ana María\AppData\Local\Exhibit\Exhibit.exe"),
            env: vec![],
        }
    }

    #[test]
    fn the_line_sends_the_agent_to_the_file() {
        let file = Path::new(r"C:\Users\Ana María\AppData\Roaming\exhibit\agent-setup.md");
        assert_eq!(
            prompt(file, None),
            r#"Read and follow the instructions in "C:\Users\Ana María\AppData\Roaming\exhibit\agent-setup.md" to set me up for Exhibit."#
        );
        assert!(prompt(file, Some("Spanish")).ends_with("for Exhibit. Talk to me in Spanish."));
    }

    #[test]
    fn the_file_names_the_program_and_registers_it_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_instructions(&installed(), &dir.path().join("exhibit")).unwrap();
        assert_eq!(path.file_name().unwrap(), INSTRUCTIONS_FILE);
        let p = std::fs::read_to_string(path).unwrap();
        let exe = r#""C:\Users\Ana María\AppData\Local\Exhibit\Exhibit.exe""#;
        assert!(p.contains(&format!("- Command: `{exe}`")));
        assert!(p.contains(&format!("claude mcp add exhibit --scope user -- {exe} mcp")));
        assert!(p.contains(&format!("cmd /c '{exe} mcp --check'")));
        assert!(
            p.contains(r#""command":"C:\\Users\\Ana María\\AppData\\Local\\Exhibit\\Exhibit.exe""#)
        );
        assert!(!p.contains("Environment"));
    }

    #[test]
    fn another_library_travels_with_the_registration() {
        let launch = Launch {
            env: vec![("EXHIBIT_DATA_DIR".into(), r"C:\dev data".into())],
            ..installed()
        };
        let p = instructions(&launch);
        assert!(p.contains(r"`EXHIBIT_DATA_DIR=C:\dev data`"));
        assert!(p.contains(r#"--scope user -e EXHIBIT_DATA_DIR="C:\dev data" -- "#));
        assert!(p.contains(r#"codex mcp add exhibit --env EXHIBIT_DATA_DIR="C:\dev data" -- "#));
        assert!(p.contains(r#""env":{"EXHIBIT_DATA_DIR":"C:\\dev data"}"#));
    }

    #[test]
    fn the_report_reads_plainly() {
        let ok = Ok(Checked {
            tools: 18,
            prompts: 3,
            counts: json!({ "movies": 1, "shows": 2, "other": 0, "needs_review": 4 }),
        });
        let r = report(&installed(), &ok);
        assert!(r.starts_with("Exhibit's MCP server works."));
        assert!(r.contains("1 film, 2 shows, 0 other, 4 in needs review"));
        let empty = Ok(Checked {
            tools: 18,
            prompts: 3,
            counts: json!({}),
        });
        assert!(report(&installed(), &empty).contains("empty so far"));
        let failed: Result<Checked, String> = Err("no library".into());
        assert!(report(&installed(), &failed).contains("did not work: no library"));
    }

    #[test]
    fn a_long_path_loses_its_prefix() {
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\C:\x\E.exe")),
            PathBuf::from(r"C:\x\E.exe")
        );
    }
}
