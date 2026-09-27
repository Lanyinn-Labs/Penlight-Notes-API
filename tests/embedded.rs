use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use penlight_notes_api::{
    api,
    client::sirius::{map_error, SiriusClient},
    config::Config,
    error::AppError,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

fn settings() -> sirius_api_proxy::config::Config {
    // Build the same environment-backed JP settings without mutating process environment.
    let mut protocol: sirius_api_proxy::config::Config = serde_json::from_value(json!({
        "region":"jp", "platform":"Android", "environment":"release",
        "endpoint":"https://api.bang-dream-on.jp", "client_version":"1.0.2",
        "protocol_directory":PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/sirius-api-proxy/protocol/sirius/1.0.3"),
        "api_token_env":"unused", "internal_token_env":"unused", "accounts":[],
        "default_cdn_root":"https://static.bang-dream-on.jp",
        "cdn_credential_env":{"https://static.bang-dream-on.jp":"PENLIGHT_CDN_PASSWORD"}
    })).unwrap();
    protocol.master_directory = None;
    protocol
}

async fn get(router: axum::Router, path: &str) -> (u16, Value) {
    let response = router
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn disabled_jp_skips_client_initialization_and_background_tasks() {
    let mut config = Config::default();
    let mut protocol = settings();
    protocol.protocol_directory = PathBuf::from("missing-disabled-region-protocol");
    config.regions[1].enabled = false;
    config.regions[1].sirius = Some(protocol);
    assert!(api::load_client(&config).unwrap().is_none());
    let router = api::build(Arc::new(config));
    let (status, health) = get(router.clone(), "/health").await;
    assert_eq!(status, 200);
    assert_eq!(health["master_update"]["status"], "disabled");
    assert_eq!(get(router, "/api/jp/application").await.0, 503);
}

#[tokio::test]
async fn embedded_protocol_initializes_without_a_proxy_listener() {
    let client = SiriusClient::new(settings()).unwrap();
    let status = serde_json::to_value(client.core.protocol_status().unwrap()).unwrap();
    assert_eq!(status["family"], "jp");
    assert_eq!(status["codec"], "native");
    assert!(!client.ready().await);
    assert_eq!(
        client
            .upstream_status(std::time::Duration::from_secs(300))
            .await["status"],
        "unknown"
    );
    let mut wrong = settings();
    wrong.region = sirius_api_proxy::region::Region::En;
    assert!(SiriusClient::new(wrong).is_err());
}

#[tokio::test]
async fn invalid_queries_and_private_access_are_rejected_before_game_dispatch() {
    let mut config = Config::default();
    config.regions[1].sirius = Some(settings());
    let router = api::build(Arc::new(config));
    for path in [
        "/api/jp/announcements?tab=3",
        "/api/jp/announcements?unknown=1",
        "/api/jp/announcements/0",
        "/api/jp/players/by-profile-id/-1",
        "/api/jp/music/0/rankings",
        "/api/jp/events/0/players/player/deck",
    ] {
        assert_eq!(get(router.clone(), path).await.0, 400, "{path}");
    }
    for path in [
        "/api/jp/user/account",
        "/api/jp/user/data",
        "/api/jp/user/decks",
    ] {
        assert_eq!(get(router.clone(), path).await.0, 401, "{path}");
    }
    assert_eq!(get(router.clone(), "/api/global/application").await.0, 501);
    assert_eq!(get(router, "/health").await.1["upstream_ready"], false);
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("penlight-embedded-{}-{n}", std::process::id()));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot(root: &std::path::Path, name: &str, version: &str, id: i64) {
    let path = root.join(name);
    fs::create_dir(&path).unwrap();
    let tables = [
        (
            "MasterMemberCard",
            json!({"_allData":[{"_id":id,"_nameTextID":"name"}]}),
        ),
        (
            "MasterText",
            json!({"_allData":[{"_id":"name","_japanese":format!("カード{id}")}]}),
        ),
    ];
    let mut files = vec![];
    let mut source = vec![];
    for (table, value) in tables {
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(path.join(format!("{table}.json")), &bytes).unwrap();
        files.push(sirius_api_proxy::master_registry::file(
            format!("{table}.json"),
            &bytes,
        ));
        source.push(json!({"name":format!("{table}.bin"),"size":96,"hash":"0".repeat(64)}));
    }
    fs::write(
        path.join("tables.json"),
        serde_json::to_vec(&json!({"schema_version":1,"version":version,"files":files})).unwrap(),
    )
    .unwrap();
    fs::write(
        path.join("MasterManifest.json"),
        serde_json::to_vec(&json!({"version":version,"files":source})).unwrap(),
    )
    .unwrap();
    fs::write(path.join("receipt.json"),serde_json::to_vec(&json!({"version":version,"snapshot":name,"tables":2,"source":"local-import","region":"jp"})).unwrap()).unwrap();
    fs::write(root.join("CURRENT"), name).unwrap();
}

#[tokio::test]
async fn master_reads_pin_versions_reject_tampering_and_enrich_resource_names() {
    let directory = Directory::new();
    snapshot(&directory.0, "master-old", "old", 1);
    let mut config = settings();
    config.master_directory = Some(directory.0.clone());
    let client = SiriusClient::new(config.clone()).unwrap();
    let old = client.master_manifest().await.unwrap();
    snapshot(&directory.0, "master-new", "new", 2);
    assert_eq!(
        client
            .master_records_at(&old, "MasterMemberCard")
            .await
            .unwrap()["entries"][0]["_id"],
        1
    );
    let cards = client.catalog("cards").await.unwrap();
    assert_eq!(cards["master_version"], "new");
    assert_eq!(cards["entries"][0]["name_ja"], "カード2");
    assert_eq!(cards["source"], "master_snapshot");
    let mut frontend = Config::default();
    frontend.regions[1].sirius = Some(config);
    let router = api::build(Arc::new(frontend));
    let (status, entry) = get(router.clone(), "/api/jp/cards/2").await;
    assert_eq!(status, 200);
    assert_eq!(entry["entry"]["id"], 2);
    assert!(entry.get("entries").is_none());
    assert_eq!(get(router.clone(), "/api/jp/cards/0").await.0, 400);
    fs::write(
        directory.0.join("master-new/MasterMemberCard.json"),
        br#"{"_allData":[]}"#,
    )
    .unwrap();
    assert!(client.catalog("cards").await.is_err());
    assert_eq!(get(router, "/api/jp/cards").await.0, 503);
}

#[test]
fn upstream_errors_preserve_business_status_without_private_diagnostics() {
    use sirius_api_proxy::error::AppError as Upstream;
    assert!(matches!(
        map_error(Upstream::Grpc(16)),
        AppError::UpstreamAuthenticationUnavailable
    ));
    assert!(matches!(
        map_error(Upstream::Grpc(8)),
        AppError::UpstreamRateLimited
    ));
    assert!(matches!(map_error(Upstream::Grpc(5)), AppError::NotFound));
    assert!(matches!(
        map_error(Upstream::Timeout),
        AppError::UpstreamTimeout
    ));
    assert!(matches!(
        map_error(Upstream::Grpc(2)),
        AppError::UpstreamGameError(2)
    ));
    assert_eq!(
        map_error(Upstream::Config("a private diagnostic")).to_string(),
        "game service returned an invalid ranking response"
    );
}

#[tokio::test]
async fn master_updater_status_is_read_only_and_missing_secrets_fail_startup() {
    let mut frontend = Config::default();
    frontend.regions[1].sirius = Some(settings());
    let router = api::build(Arc::new(frontend));
    let (status, body) = get(router, "/api/jp/master-updater").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "disabled");

    let mut config = settings();
    config.master_directory = Some(PathBuf::from("unused-test-store"));
    config.master_update = Some(sirius_api_proxy::config::MasterUpdateConfig {
        network: Default::default(),
        cdn_authorization: sirius_api_proxy::config::CdnAuthorization::Basic,
        username_env: Some("PENLIGHT_TEST_MISSING_CDN_USERNAME_672b7111".into()),
        key_hex_env: "PENLIGHT_TEST_MISSING_KEY".into(),
        iv_hex_env: "PENLIGHT_TEST_MISSING_IV".into(),
        interval_seconds: 300,
    });
    config.validate().unwrap();
    let client = SiriusClient::new(config).unwrap();
    assert_eq!(
        client.core.master_update_status().await["status"],
        "pending"
    );
    assert!(client.start_worker().is_err());
}
