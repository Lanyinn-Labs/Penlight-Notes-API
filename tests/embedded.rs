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
use std::{fs, path::PathBuf, sync::Arc};
use tower::ServiceExt;

fn settings() -> sirius_api_proxy::config::Config {
    // Build the same environment-backed JP settings without mutating process environment.
    let mut protocol: sirius_api_proxy::config::Config = serde_json::from_value(json!({
        "region":"jp", "platform":"Android", "environment":"release",
        "endpoint":"https://api.bang-dream-on.jp", "client_version":"1.0.4",
        "protocol_directory":PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/sirius-api-proxy/protocol/sirius/1.0.4"),
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
    assert_eq!(status["version"], "1.0.4");
    let bundle =
        sirius_api_proxy::protocol::ProtocolBundle::load(&settings().protocol_directory).unwrap();
    let player = bundle
        .pool
        .get_message_by_name("entity.PlayerData")
        .unwrap();
    assert_eq!(
        player.get_field(61).unwrap().json_name(),
        "characterCurrentCostumes"
    );
    assert_eq!(
        player.get_field(62).unwrap().json_name(),
        "characterUnlockedCostumes"
    );
    assert!(bundle
        .pool
        .get_message_by_name("entity.Announcement")
        .unwrap()
        .get_field_by_name("platform")
        .is_some());
    assert!(!client.ready().await);
    assert_eq!(
        client
            .upstream_status(std::time::Duration::from_secs(300))
            .await["status"],
        "unknown"
    );
    let mut config = Config::default();
    config.regions[1].sirius = Some(settings());
    let (_, version) = get(api::build(Arc::new(config)), "/version").await;
    let provenance: Value =
        serde_json::from_str(include_str!("../vendor/sirius-api-proxy/UPSTREAM.json")).unwrap();
    assert_eq!(version["upstream_version"], provenance["version"]);
    assert_eq!(version["protocol_revision"], provenance["revision"]);
    assert_eq!(version["protocol_version"], "1.0.4");
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
        "/api/jp/user/export",
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
        Self(
            tempfile::Builder::new()
                .prefix("penlight-embedded-")
                .tempdir()
                .unwrap()
                .keep(),
        )
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn temporary_directories_are_isolated_under_concurrent_creation() {
    let directories = std::thread::scope(|scope| {
        let workers = (0..32)
            .map(|_| scope.spawn(Directory::new))
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    let paths = directories
        .iter()
        .map(|directory| &directory.0)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(paths.len(), directories.len());
    for (index, directory) in directories.iter().enumerate() {
        fs::write(directory.0.join("owner"), index.to_string()).unwrap();
    }
    for (index, directory) in directories.iter().enumerate() {
        assert_eq!(
            fs::read_to_string(directory.0.join("owner")).unwrap(),
            index.to_string()
        );
    }
    let paths = directories
        .iter()
        .map(|directory| directory.0.clone())
        .collect::<Vec<_>>();
    drop(directories);
    assert!(paths.iter().all(|path| !path.exists()));
}

fn snapshot(root: &std::path::Path, name: &str, version: &str, id: i64) {
    write_snapshot(
        root,
        name,
        version,
        &[
            (
                "MasterMemberCard",
                json!({"_allData":[{"_id":id,"_nameTextID":"name"}]}),
            ),
            (
                "MasterText",
                json!({"_allData":[{"_id":"name","_japanese":format!("カード{id}")}]}),
            ),
        ],
    );
}

fn write_snapshot(root: &std::path::Path, name: &str, version: &str, tables: &[(&str, Value)]) {
    let path = root.join(name);
    fs::create_dir(&path).unwrap();
    let mut tables = tables.iter().collect::<Vec<_>>();
    tables.sort_by_key(|(name, _)| *name);
    let mut files = vec![];
    let mut source = vec![];
    for (table, value) in &tables {
        let bytes = serde_json::to_vec(value).unwrap();
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
    fs::write(path.join("receipt.json"),serde_json::to_vec(&json!({"version":version,"snapshot":name,"tables":tables.len(),"source":"local-import","region":"jp"})).unwrap()).unwrap();
    fs::write(root.join("CURRENT"), name).unwrap();
}

#[tokio::test]
async fn online_catalog_reads_text_only_when_entries_need_it() {
    let directory = Directory::new();
    write_snapshot(
        &directory.0,
        "master-no-text",
        "v1",
        &[
            ("MasterEvent", json!({"_allData":[{"_id":7}]})),
            ("MasterMemberCard", json!({"_allData":[]})),
            (
                "MasterCharacter",
                json!({"_allData":[{"_id":1,"_nameTextID":"name"}]}),
            ),
        ],
    );
    let mut config = settings();
    config.master_directory = Some(directory.0.clone());
    let client = SiriusClient::new(config.clone()).unwrap();
    assert_eq!(
        client.catalog("events").await.unwrap()["entries"][0]["id"],
        7
    );
    assert!(client.catalog("cards").await.unwrap()["entries"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(client.catalog("characters").await.is_err());

    let mut frontend = Config::default();
    frontend.regions[1].sirius = Some(config);
    let (status, events) = get(api::build(Arc::new(frontend)), "/api/jp/events").await;
    assert_eq!(status, 200);
    assert_eq!(events["entries"][0]["id"], 7);
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
        map_error(Upstream::Maintenance(14)),
        AppError::UpstreamMaintenance
    ));
    assert!(matches!(
        map_error(Upstream::UpstreamUnavailable),
        AppError::UpstreamUnavailable
    ));
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
        "game service returned an invalid response"
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

fn event_record(disabled: bool) -> Value {
    use chrono::{FixedOffset, Utc};
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap());
    json!({"_id":1,"_nameTextId":"Event_Name_0001",
        "_startAt":(now-chrono::Duration::days(1)).format("%Y/%m/%d %H:%M:%S").to_string(),
        "_endAt":(now+chrono::Duration::days(1)).format("%Y/%m/%d %H:%M:%S").to_string(),
        "_displayEndAt":(now+chrono::Duration::days(2)).format("%Y/%m/%d %H:%M:%S").to_string(),
        "_isRankingDisabled":disabled,"_isMusicRankingDisabled":false,"_isTotalMusicRankingDisabled":false})
}

struct EventRankingFixture(std::sync::atomic::AtomicUsize);
impl penlight_notes_api::ranking::RankingSource for EventRankingFixture {
    fn fetch<'a>(
        &'a self,
        _: penlight_notes_api::region::Region,
        request: &'a penlight_notes_api::ranking::RankingRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Vec<penlight_notes_api::ranking::RankingPoint>, AppError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert_eq!(request.event_id, 1);
            Ok(request
                .ranks
                .iter()
                .map(|rank| penlight_notes_api::ranking::RankingPoint {
                    rank: *rank,
                    point: if *rank == 1 { 123456 } else { 0 },
                })
                .collect())
        })
    }
}

#[tokio::test]
async fn active_event_cutoffs_use_master_capabilities_and_shared_cache() {
    let directory = Directory::new();
    write_snapshot(
        &directory.0,
        "master-event",
        "v1",
        &[("MasterEvent", json!({"_allData":[event_record(false)]}))],
    );
    let mut config = Config::default();
    let mut protocol = settings();
    protocol.master_directory = Some(directory.0.clone());
    config.regions[1].sirius = Some(protocol);
    config.ranking_default_ranks = vec![1, 100];
    let source = Arc::new(EventRankingFixture(std::sync::atomic::AtomicUsize::new(0)));
    let router = api::build_with_ranking_source(Arc::new(config), source.clone());
    let (status, event) = get(router.clone(), "/api/jp/events/current").await;
    assert_eq!(status, 200);
    assert_eq!(event["event"]["id"], 1);
    assert_eq!(event["event"]["ranking_enabled"], true);
    assert_eq!(event["master_version"], "v1");
    let (status, current) = get(router.clone(), "/api/jp/events/current/cutoffs").await;
    assert_eq!(status, 200);
    assert_eq!(current["event_id"], 1);
    assert_eq!(current["complete"], true);
    assert_eq!(current["cutoffs"][0]["point"], 123456);
    assert_eq!(current["cutoffs"][1]["point"], 0);
    let (status, explicit) = get(router.clone(), "/api/jp/events/1/cutoffs?ranks=100,1,1").await;
    assert_eq!(status, 200);
    assert_eq!(
        current["observed_at_unix_ms"],
        explicit["observed_at_unix_ms"]
    );
    assert_eq!(source.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jp/events/1/cutoffs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(
        get(router.clone(), "/api/jp/events/99/cutoffs").await.0,
        404
    );
    assert_eq!(
        get(router.clone(), "/api/jp/events/current/cutoffs?ranks=0")
            .await
            .0,
        400
    );
    assert_eq!(get(router, "/api/jp/events/1/rankings").await.0, 400);
}

#[tokio::test]
async fn disabled_event_rankings_never_fetch_or_fabricate_points() {
    let directory = Directory::new();
    write_snapshot(
        &directory.0,
        "master-event",
        "v1",
        &[("MasterEvent", json!({"_allData":[event_record(true)]}))],
    );
    let mut config = Config::default();
    let mut protocol = settings();
    protocol.master_directory = Some(directory.0.clone());
    config.regions[1].sirius = Some(protocol);
    let source = Arc::new(EventRankingFixture(std::sync::atomic::AtomicUsize::new(0)));
    let router = api::build_with_ranking_source(Arc::new(config), source.clone());
    let (status, event) = get(router.clone(), "/api/jp/events/current").await;
    assert_eq!(status, 200);
    assert_eq!(event["event"]["ranking_enabled"], false);
    assert_eq!(event["event"]["music_ranking_enabled"], true);
    for path in [
        "/api/jp/events/current/cutoffs",
        "/api/jp/events/1/cutoffs?ranks=100",
        "/api/jp/events/1/rankings?ranks=100",
    ] {
        let (status, body) = get(router.clone(), path).await;
        assert_eq!(status, 409, "{path}");
        assert_eq!(body["error"]["code"], "event_ranking_disabled");
    }
    assert_eq!(source.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    write_snapshot(
        &directory.0,
        "master-empty",
        "v2",
        &[("MasterEvent", json!({"_allData":[]}))],
    );
    assert_eq!(get(router.clone(), "/api/jp/events/current").await.0, 404);
    assert_eq!(get(router, "/api/jp/events/current/cutoffs").await.0, 404);
}

#[tokio::test]
async fn current_event_metadata_works_with_an_offline_snapshot() {
    let directory = Directory::new();
    let schema: Value =
        serde_json::from_str(include_str!("../data/master-schema-jp.json")).unwrap();
    fs::write(
        directory.0.join("summary.json"),
        serde_json::to_vec(&json!({
        "source_asset_pack_apk_sha256":schema["apk_sha256"],
        "source_base_apk_sha256":schema["base_apk_sha256"]}))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        directory.0.join("MasterEvent.json"),
        serde_json::to_vec(&json!({"_allData":[event_record(false)]})).unwrap(),
    )
    .unwrap();
    let mut config = Config::default();
    config.regions[1].master_dir = Some(directory.0.clone());
    let router = api::build(Arc::new(config.clone()));
    let (status, event) = get(router.clone(), "/api/jp/events/current").await;
    assert_eq!(status, 200);
    assert_eq!(event["event"]["id"], 1);
    assert_eq!(event["source"], "apk_master_snapshot");
    assert_eq!(get(router, "/api/jp/events/current/cutoffs").await.0, 501);
    config.regions[1].enabled = false;
    assert_eq!(
        get(api::build(Arc::new(config)), "/api/jp/events/current")
            .await
            .0,
        503
    );
}
