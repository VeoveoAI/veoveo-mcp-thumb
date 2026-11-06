//! The MCP surface through the SDK's in-process Hub: discovery, permission refusals and the skill's
//! manifest. Reading a file needs a Hub with Lore behind it, which the deployed probes cover.
use std::time::Duration;

use serde_json::{Value, json};
use veoveo_hub_sdk::hosting::{
    Hosted,
    testing::{self, TestHub},
};
use veoveo_mcp_thumb::{
    contract::ThumbPermission::{self, ThumbnailsRead},
    hub::Hub,
    mcp::ThumbMcp,
    thumbnail::Thumbnails,
};

/// A Hub that grants `granted` in front of the server. Nothing behind it answers a Hub route.
fn hub(granted: &[ThumbPermission]) -> (TestHub, tempfile::TempDir) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let http = reqwest::Client::builder().build().unwrap();
    let hub = Hub::new(
        &url::Url::parse("http://127.0.0.1:9/").unwrap(),
        http.clone(),
    )
    .unwrap();
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
            testing::for_domain::<ThumbMcp>()
                .handler(move || Hosted::new(ThumbMcp::new(hub.clone(), thumbnails.clone())))
                .build(),
        )
        .granting(granted.iter().map(|permission| permission.name())),
        folder,
    )
}

#[tokio::test]
async fn tools_are_listed_with_their_permission() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (full, _kept) = hub(&[ThumbnailsRead]);
        let tools = full.rpc("tools/list", json!({})).await;
        let mut names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{tools}"))
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["ready_thumbnails", "thumbnail"]);

        // With nothing granted, nothing is admitted.
        let (nothing, _kept) = hub(&[]);
        let refused = nothing
            .rpc(
                "tools/call",
                json!({"name": "thumbnail", "arguments": {
                    "source": {"from": "lore", "repository": "r", "branch": "main", "path": "a.png"}
                }}),
            )
            .await;
        assert!(refused["error"].is_object(), "{refused}");
    })
    .await
    .expect("the test finished in time");
}

#[tokio::test]
async fn a_path_that_climbs_is_refused_before_any_read() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (reader, _kept) = hub(&[ThumbnailsRead]);
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
        let (hub, _kept) = hub(&[ThumbnailsRead]);
        let listed = as_json(&hub.rpc("skills/list", json!({})).await);
        let skills = listed["skills"]
            .as_array()
            .unwrap_or_else(|| panic!("{listed}"));
        assert_eq!(skills.len(), 1);
        let skill = &skills[0];
        assert_eq!(skill["uri"], "skill://thumb/make-thumbnails/SKILL.md");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/make-thumbnails");
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
                    .strip_prefix("skill://thumb/make-thumbnails/")
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
