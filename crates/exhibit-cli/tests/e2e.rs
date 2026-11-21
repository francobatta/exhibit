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
            .env("EXHIBIT_TMDB_BUILTIN", "off")
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
    for step in ["identified", "probed", "rescored"] {
        assert!(scan[step].is_null(), "{step}: {scan}");
    }

    let all = cli.ok(&["search"]);
    let all = all.as_array().unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|i| i["kind"] == "other"));
    assert_eq!(cli.ok(&["search", "matrix"]).as_array().unwrap().len(), 1);

    let id = all[0]["id"].as_i64().unwrap().to_string();
    assert_eq!(cli.ok(&["loved", &id]), serde_json::json!({"ok": true}));
    let loved = cli.ok(&["search", "--loved"]);
    assert_eq!(loved.as_array().unwrap().len(), 1);
    assert_eq!(loved[0]["id"].to_string(), id);
    // Shelves are Home's, and Home leaves Other to its own wall.
    assert!(cli.ok(&["shelf", "loved"]).as_array().unwrap().is_empty());

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
    assert_eq!(status["tmdb_key"], false);
    // Detecting the tools runs each one, so they are listed apart from the status.
    assert!(status.get("tools").is_none());
    assert_eq!(cli.ok(&["tools", "list"]).as_array().unwrap().len(), 2);
}

#[test]
fn key_set_reads_the_value_from_stdin_never_from_arguments() {
    let data = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_exhibit"))
            .args(args)
            .env("EXHIBIT_DATA_DIR", data.path())
            .env("EXHIBIT_TMDB_BUILTIN", "off")
            .env("EXHIBIT_KEYRING_SERVICE", "exhibit-test-never-written")
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run exhibit")
    };
    // A value argument is refused by the parser, before anything is stored.
    let out = run(&["key", "set", "tmdb", "sekrit"]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    // Empty standard input is an error; nothing is stored.
    let out = run(&["key", "set", "tmdb"]);
    assert_eq!(out.status.code(), Some(1));
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["error"]["kind"], "invalid", "{json}");
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
    assert!(scan["rescored"].is_null(), "{scan}");
    for key in ["rescored", "thumbnails", "identify_error"] {
        assert!(scan.as_object().unwrap().contains_key(key), "{key}: {scan}");
    }
    assert!(scan["identify_error"].is_null(), "{scan}");
}

#[test]
fn review_search_without_key_names_the_key() {
    let data = tempfile::tempdir().unwrap();
    let cli = Cli { data: data.path() };
    let (code, err) = cli.run(&[
        "review", "search", "Heat", "--kind", "movie", "--year", "1995",
    ]);
    assert_eq!(code, 1, "{err}");
    let message = err["error"]["message"].as_str().unwrap();
    assert!(message.contains("TMDB key"), "{message}");
}

/// The player writes to its stdout and stderr; `exhibit play` still prints one JSON document.
#[cfg(windows)]
#[test]
fn play_prints_only_its_own_json() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    std::fs::write(media.path().join("Heat.1995.mkv"), b"").unwrap();
    let cli = Cli { data: data.path() };
    cli.ok(&["source", "add", media.path().to_str().unwrap()]);
    cli.ok(&["scan", "--no-probe", "--no-identify"]);
    let id = cli.ok(&["search"])[0]["id"].as_i64().unwrap().to_string();
    let tools = tempfile::tempdir().unwrap();
    // Tracked (named mpv) and untracked players alike.
    for name in ["mpv.bat", "vlc.bat"] {
        let player = tools.path().join(name);
        std::fs::write(
            &player,
            "@echo player noise on stdout\r\n@echo player noise on stderr 1>&2\r\n",
        )
        .unwrap();
        cli.ok(&["tools", "set", "mpv", player.to_str().unwrap()]);
        let out = Command::new(env!("CARGO_BIN_EXE_exhibit"))
            .args(["play", &id])
            .env("EXHIBIT_DATA_DIR", data.path())
            .env("EXHIBIT_TMDB_BUILTIN", "off")
            .env("EXHIBIT_KEYRING_SERVICE", "exhibit-test-never-written")
            .output()
            .expect("run exhibit");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(0), "{name}: {stdout}");
        assert_eq!(stdout.lines().count(), 1, "{name}: {stdout}");
        let json: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(json["item_id"].to_string(), id, "{name}: {json}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("player noise"), "{name}: {stderr}");
    }
}
