use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use penlight_notes_api::{
    api,
    config::Config,
    error::AppError,
    ranking::{CachePolicy, RankingPoint, RankingRequest, RankingSource},
    region::Region,
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    fs,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

async fn call(config: Config, path: &str, header: Option<(&str, &str)>) -> (u16, Value) {
    call_router(api::build(Arc::new(config)), path, header).await
}

async fn call_router(router: Router, path: &str, header: Option<(&str, &str)>) -> (u16, Value) {
    let mut request = Request::builder().uri(path);
    if let Some((name, value)) = header {
        request = request.header(name, value);
    }
    let response = router
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

struct FakeRankingSource {
    calls: AtomicUsize,
    replies: Mutex<VecDeque<Result<Vec<RankingPoint>, AppError>>>,
}

impl FakeRankingSource {
    fn new(replies: Vec<Result<Vec<RankingPoint>, AppError>>) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            replies: Mutex::new(replies.into()),
        }
    }
}

impl RankingSource for FakeRankingSource {
    fn fetch<'a>(
        &'a self,
        _region: Region,
        _request: &'a RankingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<RankingPoint>, AppError>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(AppError::UpstreamUnavailable));
        Box::pin(async move {
            tokio::task::yield_now().await;
            reply
        })
    }
}

fn test_router(source: Arc<FakeRankingSource>, policy: CachePolicy) -> Router {
    let config = Config {
        ranking_cache: policy,
        ..Config::default()
    };
    api::build_with_ranking_source(Arc::new(config), source)
}

#[tokio::test]
async fn region_states_are_explicit() {
    for (path, status, code) in [
        ("/api/global/application", 501, "protocol_pending"),
        ("/api/jp/application", 501, "protocol_pending"),
        ("/api/unknown/application", 400, "unsupported_region"),
        ("/api/global/cards", 501, "protocol_pending"),
    ] {
        let (actual, body) = call(Config::default(), path, None).await;
        assert_eq!(actual, status, "{path}");
        assert_eq!(body["error"]["code"], code);
    }
    let mut config = Config::default();
    config.regions[1].enabled = false;
    assert_eq!(call(config, "/api/jp/application", None).await.0, 503);
}

#[tokio::test]
async fn authentication_protects_api_but_not_health() {
    let config = Config {
        api_key: Some("test-key".into()),
        ..Config::default()
    };
    assert_eq!(call(config.clone(), "/health", None).await.0, 200);
    assert_eq!(
        call(config.clone(), "/api/global/application", None)
            .await
            .0,
        401
    );
    assert_eq!(
        call(
            config.clone(),
            "/api/global/application",
            Some(("x-api-key", "wrong"))
        )
        .await
        .0,
        401
    );
    for candidate in ["test-ke", "test-key-extra"] {
        assert_eq!(
            call(
                config.clone(),
                "/api/global/application",
                Some(("x-api-key", candidate))
            )
            .await
            .0,
            401
        );
    }
    for header in [
        ("x-api-key", "test-key"),
        ("authorization", "Bearer test-key"),
    ] {
        assert_eq!(
            call(config.clone(), "/api/global/application", Some(header))
                .await
                .0,
            501
        );
    }
}

#[tokio::test]
async fn resource_snapshot_uses_optional_api_key_and_online_jp_protocol() {
    let path = "/internal/v1/jp/resources/snapshot";
    assert_eq!(call(Config::default(), path, None).await.0, 501);

    let config = Config {
        api_key: Some("test-key".into()),
        ..Config::default()
    };
    assert_eq!(call(config.clone(), path, None).await.0, 401);
    assert_eq!(
        call(config, path, Some(("authorization", "Bearer test-key")))
            .await
            .0,
        501
    );
}

#[tokio::test]
async fn configured_upstream_is_not_reported_as_ready() {
    let mut config = Config::default();
    config.regions[1].sirius = Some(serde_json::from_value(serde_json::json!({
        "region":"jp", "platform":"Android", "environment":"release",
        "endpoint":"https://example.invalid", "client_version":"1.0.3",
        "protocol_directory":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/sirius-api-proxy/protocol/sirius/1.0.3"),
        "api_token_env":"unused", "internal_token_env":"unused", "accounts":[],
        "default_cdn_root":"https://static.bang-dream-on.jp",
        "cdn_credential_env":{"https://static.bang-dream-on.jp":"PENLIGHT_CDN_PASSWORD"}
    })).unwrap());
    let (status, body) = call(config, "/servers", None).await;
    assert_eq!(status, 200);
    assert_eq!(body["servers"][1]["upstream_configured"], true);
    assert_eq!(body["servers"][1]["upstream_ready"], false);
    assert!(!body.to_string().contains("example.invalid"));
}

#[tokio::test]
async fn apk_master_schemas_are_available_without_upstream_data() {
    let (status, index) = call(Config::default(), "/api/global/master-schema", None).await;
    assert_eq!(status, 200);
    assert_eq!(index["source"], "apk_il2cpp_metadata");
    assert_eq!(index["records_available"], false);
    assert_eq!(index["entries"].as_array().unwrap().len(), 240);

    let (status, detail) = call(
        Config::default(),
        "/api/global/master-schema/MasterCharacter",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(detail["table"]["model_type"], "App.Master.MasterCharacter");
    assert!(detail["table"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field["name"] == "_bandID"));
    assert_eq!(detail["records_available"], false);

    assert_eq!(
        call(Config::default(), "/api/global/master-schema/Unknown", None)
            .await
            .0,
        404
    );
    assert_eq!(
        call(Config::default(), "/api/jp/master-schema", None)
            .await
            .1["entries"]
            .as_array()
            .unwrap()
            .len(),
        235
    );
}

#[tokio::test]
async fn decrypted_master_snapshot_requires_matching_apk_provenance() {
    assert_eq!(
        call(
            Config::default(),
            "/api/global/master/MasterCharacter",
            None
        )
        .await
        .1["error"]["code"],
        "master_data_unavailable"
    );

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("penlight-master-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let schema: Value =
        serde_json::from_str(include_str!("../data/master-schema-global.json")).unwrap();
    fs::write(
        directory.join("summary.json"),
        serde_json::to_vec(&serde_json::json!({"source_apk_sha256": schema["apk_sha256"]}))
            .unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("MasterCharacter.json"),
        br#"{"_allData":[{"_id":1,"_nameTextID":"Character_Name_Tomori"}]}"#,
    )
    .unwrap();

    let mut config = Config::default();
    config.regions[0].master_dir = Some(directory.clone());
    let (status, body) = call(config.clone(), "/api/global/master/MasterCharacter", None).await;
    assert_eq!(status, 200);
    assert_eq!(body["source"], "apk_master_snapshot");
    assert_eq!(body["entries"][0]["_id"], 1);
    assert_eq!(
        call(config.clone(), "/api/global/master/Unknown", None)
            .await
            .0,
        404
    );

    fs::write(
        directory.join("summary.json"),
        br#"{"source_apk_sha256":"wrong"}"#,
    )
    .unwrap();
    assert_eq!(
        call(config, "/api/global/master/MasterCharacter", None)
            .await
            .0,
        503
    );
    fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn jp_snapshot_uses_jp_tables_and_checks_both_apk_parts() {
    let schema: Value =
        serde_json::from_str(include_str!("../data/master-schema-jp.json")).unwrap();
    let (status, index) = call(Config::default(), "/api/jp/master-schema", None).await;
    assert_eq!(status, 200);
    assert_eq!(index["region"], "jp");
    assert_eq!(index["entries"].as_array().unwrap().len(), 235);

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "penlight-jp-master-{}-{suffix}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("summary.json"),
        serde_json::to_vec(&serde_json::json!({
            "source_base_apk_sha256": schema["base_apk_sha256"],
            "source_asset_pack_apk_sha256": schema["apk_sha256"]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("MasterCharacter.json"),
        br#"{"_allData":[{"_id":1}]}"#,
    )
    .unwrap();
    fs::write(
        directory.join("MasterText.json"),
        r#"{"_allData":[{"_id":"Character_Name_Tomori","_japanese":"燈"},{"_id":"Card_Subtitle_7","_japanese":"星の歌"}]}"#,
    )
    .unwrap();
    fs::write(
        directory.join("MasterMemberCard.json"),
        br#"{"_allData":[{"_id":7,"_nameTextID":"Character_Name_Tomori","_subtitleTextID":"Card_Subtitle_7"}]}"#,
    )
    .unwrap();
    fs::write(directory.join("MasterEvent.json"), br#"{"_allData":[]}"#).unwrap();
    let mut config = Config::default();
    config.regions[1].master_dir = Some(directory.clone());
    let (status, body) = call(config.clone(), "/api/jp/master/MasterCharacter", None).await;
    assert_eq!(status, 200);
    assert_eq!(body["region"], "jp");
    assert_eq!(body["entries"][0]["_id"], 1);
    let (status, cards) = call(config.clone(), "/api/jp/cards", None).await;
    assert_eq!(status, 200);
    assert_eq!(cards["table"], "MasterMemberCard");
    assert_eq!(cards["entries"][0]["id"], 7);
    assert_eq!(cards["entries"][0]["name_ja"], "燈");
    assert_eq!(cards["entries"][0]["subtitle_ja"], "星の歌");
    let (status, card) = call(config.clone(), "/api/jp/cards/7", None).await;
    assert_eq!(status, 200);
    assert_eq!(card["entry"]["_id"], 7);
    assert_eq!(call(config.clone(), "/api/jp/cards/8", None).await.0, 404);
    assert_eq!(call(config.clone(), "/api/jp/cards/0", None).await.0, 400);
    let (status, events) = call(config.clone(), "/api/jp/events", None).await;
    assert_eq!(status, 200);
    assert_eq!(events["entries"].as_array().unwrap().len(), 0);
    assert_eq!(
        call(config.clone(), "/api/jp/master/Unknown", None).await.0,
        404
    );
    fs::write(
        directory.join("summary.json"),
        br#"{"source_base_apk_sha256":"wrong"}"#,
    )
    .unwrap();
    assert_eq!(
        call(config, "/api/jp/master/MasterCharacter", None).await.0,
        503
    );
    fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn cutoff_route_validates_requests_and_never_invents_live_scores() {
    let path = "/api/global/events/42/cutoffs?ranks=100,1000";
    let (status, body) = call(Config::default(), path, None).await;
    assert_eq!(status, 501);
    assert_eq!(body["error"]["code"], "protocol_pending");

    for path in [
        "/api/global/events/0/cutoffs?ranks=100",
        "/api/global/events/42/cutoffs?ranks=",
        "/api/global/events/42/cutoffs?ranks=0",
        "/api/global/events/42/cutoffs?ranks=100,nope",
    ] {
        let (status, body) = call(Config::default(), path, None).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["error"]["code"], "invalid_ranking_query");
    }
    assert_eq!(
        call(
            Config::default(),
            "/api/jp/events/42/cutoffs?ranks=100",
            None
        )
        .await
        .0,
        501
    );
    assert_eq!(
        call(
            Config::default(),
            "/api/unknown/events/42/cutoffs?ranks=100",
            None
        )
        .await
        .0,
        400
    );
}

#[tokio::test]
async fn cutoff_cache_merges_identical_requests_and_preserves_observation_time() {
    let source = Arc::new(FakeRankingSource::new(vec![Ok(vec![RankingPoint {
        rank: 100,
        point: 12345,
    }])]));
    let router = test_router(
        source.clone(),
        CachePolicy {
            fresh: Duration::from_secs(60),
            stale: Duration::from_secs(120),
            retry: Duration::from_secs(1),
        },
    );
    let path = "/api/global/events/42/cutoffs?ranks=1000,100,100";
    let (first, second) = tokio::join!(
        call_router(router.clone(), path, None),
        call_router(router.clone(), path, None)
    );
    assert_eq!(first.0, 200);
    assert_eq!(second.0, 200);
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.1["source"], "official_game_service");
    assert_eq!(first.1["status"], "fresh");
    assert_eq!(first.1["complete"], false);
    assert_eq!(first.1["cutoffs"][0]["rank"], 100);
    assert_eq!(first.1["cutoffs"][0]["point"], 12345);
    assert_eq!(first.1["cutoffs"][1]["rank"], 1000);
    assert!(first.1["cutoffs"][1]["point"].is_null());
    assert_eq!(
        first.1["observed_at_unix_ms"],
        second.1["observed_at_unix_ms"]
    );
}

#[tokio::test]
async fn cutoff_cache_marks_old_values_stale_then_stops_serving_them() {
    let source = Arc::new(FakeRankingSource::new(vec![
        Ok(vec![RankingPoint {
            rank: 100,
            point: 5000,
        }]),
        Err(AppError::UpstreamUnavailable),
    ]));
    let service = penlight_notes_api::ranking::RankingService::new(
        source.clone(),
        CachePolicy {
            fresh: Duration::ZERO,
            stale: Duration::from_secs(60),
            retry: Duration::from_secs(60),
        },
    );
    let request = RankingRequest::parse("42", "100").unwrap();
    let first = service.get(Region::Global, request.clone()).await.unwrap();
    let second = service.get(Region::Global, request.clone()).await.unwrap();
    let third = service.get(Region::Global, request).await.unwrap();
    let first = serde_json::to_value(first).unwrap();
    let second = serde_json::to_value(second).unwrap();
    let third = serde_json::to_value(third).unwrap();
    assert_eq!(first["status"], "fresh");
    assert_eq!(second["status"], "stale");
    assert_eq!(third["status"], "stale");
    assert_eq!(first["observed_at_unix_ms"], second["observed_at_unix_ms"]);
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);

    let expired = penlight_notes_api::ranking::RankingService::new(
        Arc::new(FakeRankingSource::new(vec![
            Ok(vec![RankingPoint {
                rank: 100,
                point: 5000,
            }]),
            Err(AppError::UpstreamUnavailable),
        ])),
        CachePolicy {
            fresh: Duration::ZERO,
            stale: Duration::ZERO,
            retry: Duration::ZERO,
        },
    );
    let request = RankingRequest::parse("42", "100").unwrap();
    assert!(expired.get(Region::Global, request.clone()).await.is_ok());
    let error = expired.get(Region::Global, request).await.err().unwrap();
    assert!(matches!(error, AppError::UpstreamUnavailable));
}

#[tokio::test]
async fn invalid_upstream_ranking_data_is_rejected() {
    let source = Arc::new(FakeRankingSource::new(vec![Ok(vec![RankingPoint {
        rank: 999,
        point: 123,
    }])]));
    let router = test_router(
        source,
        CachePolicy {
            fresh: Duration::from_secs(30),
            stale: Duration::from_secs(300),
            retry: Duration::from_secs(5),
        },
    );
    let (status, body) = call_router(router, "/api/global/events/42/cutoffs?ranks=100", None).await;
    assert_eq!(status, 502);
    assert_eq!(body["error"]["code"], "upstream_invalid_response");
}

struct WaitingRankingSource {
    entered: tokio::sync::Notify,
}
impl RankingSource for WaitingRankingSource {
    fn fetch<'a>(
        &'a self,
        _: Region,
        _: &'a RankingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<RankingPoint>, AppError>> + Send + 'a>> {
        Box::pin(async move {
            self.entered.notify_one();
            std::future::pending().await
        })
    }
}

#[tokio::test]
async fn rate_limit_follows_authentication_and_exempts_health() {
    let mut config = Config {
        api_key: Some("key".into()),
        ..Config::default()
    };
    config.request_limits.per_second = 1;
    config.request_limits.burst = 1;
    let router = api::build(Arc::new(config));
    assert_eq!(
        call_router(router.clone(), "/api/jp/application", None)
            .await
            .0,
        401
    );
    assert_eq!(
        call_router(
            router.clone(),
            "/api/jp/application",
            Some(("X-API-Key", "key"))
        )
        .await
        .0,
        501
    );
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jp/application")
                .header("X-API-Key", "key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(response.headers()["retry-after"], "1");
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "api_rate_limited");
    assert_eq!(call_router(router, "/health", None).await.0, 200);
}

#[tokio::test]
async fn concurrency_rejects_without_queueing_and_cancellation_releases_capacity() {
    let source = Arc::new(WaitingRankingSource {
        entered: tokio::sync::Notify::new(),
    });
    let mut config = Config::default();
    config.request_limits.max_concurrent = 1;
    let router = api::build_with_ranking_source(Arc::new(config), source.clone());
    let pending = tokio::spawn(call_router(
        router.clone(),
        "/api/jp/events/1/cutoffs?ranks=1",
        None,
    ));
    tokio::time::timeout(Duration::from_secs(1), source.entered.notified())
        .await
        .unwrap();
    let (status, body) = call_router(router.clone(), "/api/jp/master-schema", None).await;
    assert_eq!(status, 429);
    assert_eq!(body["error"]["code"], "api_busy");
    assert_eq!(call_router(router.clone(), "/health", None).await.0, 200);
    pending.abort();
    let _ = pending.await;
    assert_eq!(
        call_router(router, "/api/jp/master-schema", None).await.0,
        200
    );
}

#[tokio::test]
async fn total_timeout_returns_json_and_releases_capacity() {
    let source = Arc::new(WaitingRankingSource {
        entered: tokio::sync::Notify::new(),
    });
    let mut config = Config::default();
    config.request_limits.max_concurrent = 1;
    config.request_limits.timeout = Duration::from_millis(30);
    let router = api::build_with_ranking_source(Arc::new(config), source);
    let (status, body) =
        call_router(router.clone(), "/api/jp/events/1/cutoffs?ranks=1", None).await;
    assert_eq!(status, 504);
    assert_eq!(body["error"]["code"], "api_timeout");
    assert_eq!(
        call_router(router, "/api/jp/master-schema", None).await.0,
        200
    );
}
