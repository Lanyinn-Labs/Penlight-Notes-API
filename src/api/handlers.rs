use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

use super::SharedState;
use crate::{client::OurNotesClient, error::AppError, region::Region};

pub async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "penlight-notes-api", "upstream_ready": false}))
}

pub async fn version() -> Json<Value> {
    Json(
        json!({"name": env!("CARGO_PKG_NAME"), "version": env!("CARGO_PKG_VERSION"), "stage": "scaffold"}),
    )
}

pub async fn servers(State(config): State<SharedState>) -> Json<Value> {
    let servers: Vec<Value> = config
        .regions
        .iter()
        .map(|region| {
            json!({
                "region": region.region,
                "enabled": region.enabled,
                "client_version": region.client_version,
                "upstream_configured": region.base_url.is_some(),
                "upstream_ready": false,
                "status": if region.enabled { "protocol_pending" } else { "disabled" },
            })
        })
        .collect();
    Json(json!({"servers": servers}))
}

pub async fn application(
    State(config): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    OurNotesClient::application(config.region(region))
        .await
        .map(Json)
}
