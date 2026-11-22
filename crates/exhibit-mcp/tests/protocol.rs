//! The server over a real temporary library, spoken to through rmcp's own client.

use exhibit_core::Library;
use exhibit_core::secrets::MemorySecrets;
use exhibit_core::tools::Env;
use exhibit_core::volume::Volumes;
use exhibit_mcp::Exhibit;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

struct Drive(PathBuf);

impl Volumes for Drive {
    fn id_of(&self, path: &Path) -> Option<String> {
        path.starts_with(&self.0).then(|| "VOL1".to_string())
    }
    fn find(&self, volume_id: &str) -> Option<PathBuf> {
        (volume_id == "VOL1").then(|| self.0.clone())
    }
}

fn never() -> exhibit_core::Result<Library> {
    unreachable!("no play in these tests")
}

/// Three films with files, one with keywords and a director, in a temporary library.
fn library() -> (tempfile::TempDir, tempfile::TempDir, Library) {
    let data = tempfile::tempdir().unwrap();
    let drive = tempfile::tempdir().unwrap();
    let media = drive.path().join("media");
    std::fs::create_dir_all(&media).unwrap();
    let lib = Library::open(
        data.path().to_path_buf(),
        Box::new(Drive(drive.path().to_path_buf())),
        Box::new(MemorySecrets::default()),
        Env::default(),
    )
    .unwrap();
    let src = lib.add_source(&media).unwrap().id;
    let c = exhibit_core::db::open(data.path()).unwrap();
    for (title, year, people, keywords) in [
        (
            "Heat",
            1995,
            r#"[{"name":"Michael Mann","role":"director"}]"#,
            vec!["heist", "los angeles"],
        ),
        (
            "Thief",
            1981,
            r#"[{"name":"Michael Mann","role":"director"}]"#,
            vec!["heist"],
        ),
        ("Amélie", 2001, "[]", vec!["paris"]),
    ] {
        c.execute(
            "INSERT INTO items(kind, title, year, people, genres, match_state, added_at) VALUES ('movie', ?1, ?2, ?3, '[\"Crime\"]', 'matched', 0)",
            (title, year, people),
        )
        .unwrap();
        let id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO files(source_id, rel_path, item_id) VALUES (?1, ?2, ?3)",
            (src, format!("{title}.mkv"), id),
        )
        .unwrap();
        for k in keywords {
            c.execute(
                "INSERT INTO item_keywords(item_id, keyword) VALUES (?1, ?2)",
                (id, k),
            )
            .unwrap();
        }
    }
    (data, drive, lib)
}

fn text(r: &CallToolResult) -> String {
    r.content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

#[tokio::test(flavor = "current_thread")]
async fn an_agent_explores_and_hangs_an_exhibition() {
    let (_data, _drive, lib) = library();
    let (server_io, client_io) = tokio::io::duplex(1 << 16);
    tokio::spawn(async move {
        Exhibit::new(lib, never)
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ().serve(client_io).await.unwrap();
    let call = |name: &'static str, args: Value| {
        let client = &client;
        async move {
            client
                .call_tool(
                    CallToolRequestParams::new(name)
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await
                .unwrap()
        }
    };

    let tools: Vec<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    for want in [
        "library_overview",
        "search_library",
        "get_work",
        "similar_works",
        "hang_exhibition",
        "play",
    ] {
        assert!(
            tools.contains(&want.to_string()),
            "{want} missing from {tools:?}"
        );
    }
    let prompts: Vec<String> = client
        .list_all_prompts()
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.name.to_string())
        .collect();
    assert_eq!(prompts.len(), 3, "{prompts:?}");

    let overview: Value =
        serde_json::from_str(&text(&call("library_overview", json!({})).await)).unwrap();
    assert_eq!(overview["counts"]["movies"], 3);
    assert_eq!(overview["directors"][0], json!(["Michael Mann", 2]));
    assert_eq!(overview["keywords"][0], json!(["heist", 2]));

    let found: Value = serde_json::from_str(&text(
        &call(
            "search_library",
            json!({ "keywords": ["heist"], "sort": "year" }),
        )
        .await,
    ))
    .unwrap();
    let titles: Vec<&str> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["title"].as_str().unwrap())
        .collect();
    // A year sort is newest first unless told otherwise.
    assert_eq!(titles, ["Heat", "Thief"]);
    // Brief records leave out what is false or empty.
    assert!(found[0].get("watched").is_none());
    let ids: Vec<i64> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_i64().unwrap())
        .collect();

    let made_up = call("hang_exhibition", json!({ "title": "Mann", "wall_text": "", "works": [{ "id": ids[0] }, { "id": ids[1] }, { "id": 9999 }] })).await;
    assert_eq!(made_up.is_error, Some(true));
    assert!(text(&made_up).contains("9999"));

    let all: Value = serde_json::from_str(&text(&call("search_library", json!({})).await)).unwrap();
    let works: Vec<Value> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|w| json!({ "id": w["id"], "note": "here" }))
        .collect();
    let hung = call("hang_exhibition", json!({ "title": "Three films", "wall_text": "A test.", "works": works, "curator": "Claude" })).await;
    assert_eq!(hung.is_error, Some(false), "{}", text(&hung));
    let shown: Value =
        serde_json::from_str(&text(&call("list_exhibitions", json!({})).await)).unwrap();
    assert_eq!(shown[0]["title"], "Three films");
    assert_eq!(shown[0]["works"].as_array().unwrap().len(), 3);

    let one: Value =
        serde_json::from_str(&text(&call("get_work", json!({ "id": ids[0] })).await)).unwrap();
    assert_eq!(one["director"], json!(["Michael Mann"]));
    assert_eq!(one["keywords"].as_array().unwrap().len(), 2);
}
