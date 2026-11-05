//! The MCP surface through the SDK's in-process Hub: discovery, permission refusals, the App and the
//! skill's manifest. Reading a file needs a Hub with Lore behind it, which the deployed probes cover.
use std::time::Duration;

use serde_json::{Value, json};
use veoveo_hub_sdk::{
    apps::AppHtml,
    hosting::{
        Hosted,
        testing::{self, TestHub},
    },
};
use veoveo_mcp_files::{
    contract::FilesPermission::{self, EntriesRead},
    hub::Hub,
    mcp::FilesMcp,
    thumbnail::Thumbnails,
};

fn app_html() -> AppHtml {
    AppHtml::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/browser.html"))
        .expect("the App's document")
}

/// A Hub that grants `granted` in front of the server. Nothing behind it answers a Hub route.
fn hub(granted: &[FilesPermission]) -> (TestHub, tempfile::TempDir) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let http = reqwest::Client::builder().build().unwrap();
    let hub = Hub::new(
        &url::Url::parse("http://127.0.0.1:9/").unwrap(),
        http.clone(),
    )
    .unwrap();
    let app = app_html();
    let folder = tempfile::tempdir().unwrap();
    let thumbnails = Thumbnails::new(
        folder.path().join("kept"),
        folder.path().join("work"),
        std::path::PathBuf::from("/bin/false"),
        http,
    )
    .unwrap();
    (
        TestHub::new(
            testing::for_domain::<FilesMcp>()
                .handler(move || {
                    Hosted::new(FilesMcp::new(hub.clone(), app.clone(), thumbnails.clone()))
                })
                .build(),
        )
        .granting(granted.iter().map(|permission| permission.name())),
        folder,
    )
}

#[tokio::test]
async fn tools_are_listed_with_their_permission() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (full, _kept) = hub(&[EntriesRead]);
        let tools = full.rpc("tools/list", json!({})).await;
        let mut names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{tools}"))
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["list_files", "thumbnail"]);

        // With nothing granted, nothing is admitted.
        let (nothing, _kept) = hub(&[]);
        let refused = nothing
            .rpc("tools/call", json!({"name": "list_files", "arguments": {}}))
            .await;
        assert!(refused["error"].is_object(), "{refused}");
    })
    .await
    .expect("the test finished in time");
}

#[tokio::test]
async fn a_path_that_climbs_is_refused_before_any_read() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (reader, _kept) = hub(&[EntriesRead]);
        for arguments in [
            json!({"repository": "r", "path": "../other"}),
            json!({"repository": "r", "path": "a/../../b"}),
        ] {
            let refused = reader
                .rpc("tools/call", json!({"name": "list_files", "arguments": arguments}))
                .await;
            assert_eq!(refused["result"]["isError"], true, "{refused}");
        }
        let refused = reader
            .rpc(
                "tools/call",
                json!({"name": "thumbnail", "arguments": {
                    "source": {"from": "lore", "repository": "r", "branch": "main", "path": "../x.png"}
                }}),
            )
            .await;
        assert_eq!(refused["result"]["isError"], true, "{refused}");
    })
    .await
    .expect("the test finished in time");
}

#[tokio::test]
async fn the_app_is_a_gallery_through_the_bridge() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (reader, _kept) = hub(&[EntriesRead]);
        let resources = reader.rpc("resources/list", json!({})).await;
        let app = resources["result"]["resources"]
            .as_array()
            .unwrap_or_else(|| panic!("{resources}"))
            .iter()
            .find(|resource| resource["uri"] == "ui://files/browser")
            .expect("the App is listed");
        assert_eq!(app["mimeType"], "text/html;profile=mcp-app");
        let document = reader
            .rpc("resources/read", json!({"uri": "ui://files/browser"}))
            .await;
        let html = document["result"]["contents"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{document}"));
        for needle in [
            "ui/initialize",
            "list_files",
            "\"thumbnail\"",
            "ui/open-link",
            "\"view\", \"lore\"",
        ] {
            assert!(html.contains(needle), "{needle}");
        }
        let tools = reader.rpc("tools/list", json!({})).await;
        for name in ["list_files", "thumbnail"] {
            let tool = tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap();
            assert_eq!(tool["_meta"]["ui"]["resourceUri"], "ui://files/browser");
        }
    })
    .await
    .expect("the test finished in time");
}

/// Every file under `dir`, as its path relative to `root`, `/`-separated.
fn files(root: &std::path::Path, dir: &std::path::Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(root, &path, found);
        } else {
            let relative = path.strip_prefix(root).unwrap();
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn as_json(response: &Value) -> Value {
    response["result"].clone()
}

#[tokio::test]
async fn the_skills_manifest_names_every_file() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (hub, _kept) = hub(&[EntriesRead]);
        let listed = as_json(&hub.rpc("skills/list", json!({})).await);
        let skills = listed["skills"]
            .as_array()
            .unwrap_or_else(|| panic!("{listed}"));
        assert_eq!(skills.len(), 1);
        let skill = &skills[0];
        assert_eq!(skill["uri"], "skill://files/browse-files/SKILL.md");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/browse-files");
        let mut on_disk = Vec::new();
        files(&root, &root, &mut on_disk);
        on_disk.sort();
        let manifest = skill["resources"].as_array().expect("a full manifest");
        let mut listed: Vec<String> = manifest
            .iter()
            .map(|file| {
                file["uri"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("skill://files/browse-files/")
                    .unwrap()
                    .to_owned()
            })
            .collect();
        listed.sort();
        assert_eq!(listed, on_disk);
    })
    .await
    .expect("the test finished in time");
}
