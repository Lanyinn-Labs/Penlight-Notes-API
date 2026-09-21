use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use penlight_notes_api::{api, config::Config};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

async fn call(config: Config, path: &str, header: Option<(&str, &str)>) -> (u16, Value) {
    let mut request = Request::builder().uri(path);
    if let Some((name, value)) = header {
        request = request.header(name, value);
    }
    let response = api::build(Arc::new(config))
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn region_states_are_explicit() {
    for (path, status, code) in [
        ("/api/global/application", 501, "protocol_pending"),
        ("/api/jp/application", 503, "region_disabled"),
        ("/api/unknown/application", 400, "unsupported_region"),
        ("/api/global/cards", 404, "not_found"),
    ] {
        let (actual, body) = call(Config::default(), path, None).await;
        assert_eq!(actual, status, "{path}");
        assert_eq!(body["error"]["code"], code);
    }
    let mut config = Config::default();
    config.regions[1].enabled = true;
    assert_eq!(call(config, "/api/jp/application", None).await.0, 501);
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
async fn configured_upstream_is_not_reported_as_ready() {
    let mut config = Config::default();
    config.regions[0].base_url = Some("https://example.invalid/secret".into());
    let (status, body) = call(config, "/servers", None).await;
    assert_eq!(status, 200);
    assert_eq!(body["servers"][0]["upstream_configured"], true);
    assert_eq!(body["servers"][0]["upstream_ready"], false);
    assert!(!body.to_string().contains("example.invalid"));
}
