use axum::{
    extract::{rejection::QueryRejection, Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sirius_api_proxy::peer::Operation;

use super::SharedState;
use crate::{
    client::sirius::{envelope, SiriusClient},
    error::AppError,
    offline_master,
    region::Region,
};

async fn upstream_status(state: &SharedState) -> Value {
    match &state.sirius {
        Some(client) if state.config.region(Region::Jp).enabled => {
            client
                .upstream_status(state.config.upstream_status_ttl)
                .await
        }
        _ => json!({"configured":false,"status":"disabled","last_observed_at":null,"age_ms":null}),
    }
}

pub async fn health(State(state): State<SharedState>) -> Json<Value> {
    let upstream = upstream_status(&state).await;
    let master_update = match &state.sirius {
        Some(client) => {
            let raw = client.core.master_update_status().await;
            // Health is public: expose task state, never account data or private diagnostics.
            json!({"status":raw["status"], "started_at":raw["started_at"], "completed_at":raw["completed_at"]})
        }
        None => json!({"status":"disabled"}),
    };
    Json(json!({"status":"ok", "service":"penlight-notes-api",
        "upstream_ready":upstream["status"] == "available", "upstream":upstream,
        "master_update":master_update}))
}

pub async fn version(State(state): State<SharedState>) -> Json<Value> {
    let upstream = crate::upstream::provenance();
    let protocol = state
        .sirius
        .as_ref()
        .and_then(|client| client.core.protocol_status().ok());
    Json(
        json!({"name": env!("CARGO_PKG_NAME"), "version": env!("CARGO_PKG_VERSION"),
        "stage": "online", "protocol_implementation":"sirius-api-proxy",
        "upstream_version":upstream.version, "protocol_revision":upstream.revision,
        "protocol_version":protocol.map(|status| status.version)}),
    )
}

pub async fn servers(State(config): State<SharedState>) -> Json<Value> {
    let upstream = upstream_status(&config).await;
    let servers: Vec<Value> = config
        .config
        .regions
        .iter()
        .map(|region| {
            json!({
                "region": region.region,
                "enabled": region.enabled,
                "client_version": region.sirius.as_ref().map(|protocol| &protocol.client_version),
                "upstream_configured": region.sirius.is_some(),
                "upstream_ready": region.region == Region::Jp && region.enabled && upstream["status"] == "available",
                "status": if !region.enabled { "disabled" } else if region.sirius.is_some() { "protocol_configured" } else { "protocol_pending" },
            })
        })
        .collect();
    Json(json!({"servers": servers}))
}

pub async fn resource_snapshot(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    online(&state, Region::parse(&region)?)?
        .resource_snapshot()
        .await
        .map(Json)
}

pub async fn application(
    State(config): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    online(&config, region)?
        .application()
        .await
        .map(envelope)
        .map(Json)
}

pub(super) fn online(state: &SharedState, region: Region) -> Result<&SiriusClient, AppError> {
    if !state.config.region(region).enabled {
        return Err(AppError::RegionDisabled);
    }
    if region != Region::Jp {
        return Err(AppError::ProtocolPending);
    }
    state.sirius.as_deref().ok_or(AppError::ProtocolPending)
}

fn private_online(state: &SharedState, region: Region) -> Result<&SiriusClient, AppError> {
    // Private account queries require configured frontend authentication even if
    // the operator deliberately leaves public resource routes unauthenticated.
    if state.config.api_key.is_none() {
        return Err(AppError::Unauthorized);
    }
    online(state, region)
}

pub(super) fn positive(raw: &str) -> Result<i64, AppError> {
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or(AppError::InvalidQuery)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnouncementQuery {
    #[serde(default)]
    tab: i32,
}

pub async fn announcements(
    State(state): State<SharedState>,
    Path(region): Path<String>,
    query: Result<Query<AnnouncementQuery>, QueryRejection>,
) -> Result<Json<Value>, AppError> {
    let Query(query) = query.map_err(|_| AppError::InvalidQuery)?;
    online(&state, Region::parse(&region)?)?
        .query(Operation::Announcements { tab: query.tab })
        .await
        .map(envelope)
        .map(Json)
}

pub async fn announcement(
    State(state): State<SharedState>,
    Path((region, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    online(&state, Region::parse(&region)?)?
        .query(Operation::Announcement { id: positive(&id)? })
        .await
        .map(envelope)
        .map(Json)
}

pub async fn player_profile(
    State(state): State<SharedState>,
    Path((region, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let client = online(&state, Region::parse(&region)?)?;
    let raw = client
        .query(Operation::Profile {
            profile_id: positive(&id)?,
        })
        .await?;
    let summary = client.profile_summary(&raw).await;
    let mut response = envelope(raw);
    response["summary"] = summary;
    Ok(Json(response))
}

pub async fn music_rankings(
    State(state): State<SharedState>,
    Path((region, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    online(&state, Region::parse(&region)?)?
        .query(Operation::MusicRanking {
            music_id: positive(&id)?,
        })
        .await
        .map(envelope)
        .map(Json)
}

pub async fn challenge_rankings(
    State(state): State<SharedState>,
    Path((region, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    online(&state, Region::parse(&region)?)?
        .query(Operation::ChallengeRanking {
            challenge_music_id: positive(&id)?,
        })
        .await
        .map(envelope)
        .map(Json)
}

pub async fn account(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let data = private_online(&state, Region::parse(&region)?)?
        .whoami()
        .await?;
    if !data["playerId"].as_str().is_some_and(|id| !id.is_empty()) {
        return Err(AppError::UpstreamInvalidResponse);
    }
    Ok(Json(envelope(json!({"authenticated":true}))))
}

pub async fn user_data(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    private_online(&state, Region::parse(&region)?)?
        .account_data()
        .await
        .map(envelope)
        .map(Json)
}

pub async fn user_export(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<(axum::http::HeaderMap, Json<Value>), AppError> {
    let data = private_online(&state, Region::parse(&region)?)?
        .account_data_shared()
        .await?;
    let snapshot = crate::account_export::snapshot(
        &data,
        &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )?;
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    Ok((headers, Json(snapshot)))
}

pub const USER_RESOURCES: &[(&str, &str)] = &[
    ("profile", "myProfile"),
    ("decks", "decks"),
    ("cards", "memberCards"),
    ("support-cards", "supportCards"),
    ("items", "items"),
    ("stamps", "stamps"),
    ("characters", "characterRank"),
    ("character-costumes", "characterCurrentCostumes"),
    ("unlocked-costumes", "characterUnlockedCostumes"),
    ("music-scores", "liveScore"),
    ("music", "liveMusic"),
    ("missions", "playerMissionData"),
    ("login-bonuses", "loginBonusUpdate"),
    ("gacha", "gachaCount"),
    ("events", "events"),
    ("tutorial", "tutorialProgress"),
];

pub async fn user_resource(
    state: SharedState,
    region: String,
    field: &'static str,
) -> Result<Json<Value>, AppError> {
    let data = private_online(&state, Region::parse(&region)?)?
        .account_data_shared()
        .await?;
    let value = data["playerData"].get(field).cloned().unwrap_or_else(|| {
        if matches!(
            field,
            "myProfile" | "playerMissionData" | "tutorialProgress"
        ) {
            Value::Null
        } else {
            json!([])
        }
    });
    Ok(Json(envelope(value)))
}

pub async fn master_schema_list(
    State(config): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    if !config.config.region(region).enabled {
        return Err(AppError::RegionDisabled);
    }
    Ok(Json(offline_master::list(region)))
}

pub async fn master_schema(
    State(config): State<SharedState>,
    Path((region, table)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    if !config.config.region(region).enabled {
        return Err(AppError::RegionDisabled);
    }
    offline_master::get(region, &table).map(Json)
}

pub async fn master_records(
    State(config): State<SharedState>,
    Path((region, table)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    let settings = config.config.region(region);
    if !settings.enabled {
        return Err(AppError::RegionDisabled);
    }
    if region == Region::Jp {
        if let Some(client) = config
            .sirius
            .as_ref()
            .filter(|client| client.master_configured())
        {
            return client.master_records(&table).await.map(Json);
        }
    }
    let directory = settings
        .master_dir
        .as_deref()
        .ok_or(AppError::MasterDataUnavailable)?;
    offline_master::records(region, directory, &table)
        .await
        .map(Json)
}

pub async fn jp_catalog_list(
    state: SharedState,
    region: String,
    resource: &'static str,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    let settings = state.config.region(region);
    if !settings.enabled {
        return Err(AppError::RegionDisabled);
    }
    if region != Region::Jp {
        return Err(AppError::ProtocolPending);
    }
    if let Some(client) = state
        .sirius
        .as_ref()
        .filter(|client| client.master_configured())
    {
        return client.catalog(resource).await.map(Json);
    }
    let directory = settings
        .master_dir
        .as_deref()
        .ok_or(AppError::MasterDataUnavailable)?;
    offline_master::jp_catalog(directory, resource)
        .await
        .map(Json)
}

pub async fn jp_catalog_entry(
    state: SharedState,
    region: String,
    resource: &'static str,
    id: String,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    let settings = state.config.region(region);
    if !settings.enabled {
        return Err(AppError::RegionDisabled);
    }
    if region != Region::Jp {
        return Err(AppError::ProtocolPending);
    }
    if let Some(client) = state
        .sirius
        .as_ref()
        .filter(|client| client.master_configured())
    {
        let id = positive(&id).map_err(|_| AppError::InvalidMasterId)?;
        let mut data = client.catalog(resource).await?;
        let entries = data
            .as_object_mut()
            .and_then(|map| map.remove("entries"))
            .ok_or(AppError::MasterDataUnavailable)?;
        let entry = entries
            .as_array()
            .ok_or(AppError::MasterDataUnavailable)?
            .iter()
            .find(|entry| {
                entry["_id"].as_i64() == Some(id)
                    || entry["_id"].as_str().and_then(|v| v.parse::<i64>().ok()) == Some(id)
            })
            .cloned()
            .ok_or(AppError::NotFound)?;
        data["entry"] = entry;
        return Ok(Json(data));
    }
    let directory = settings
        .master_dir
        .as_deref()
        .ok_or(AppError::MasterDataUnavailable)?;
    offline_master::jp_catalog_entry(directory, resource, &id)
        .await
        .map(Json)
}

pub async fn master_data(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let manifest = online(&state, Region::parse(&region)?)?
        .master_manifest()
        .await?;
    Ok(Json(
        json!({"region":"jp", "source":"master_snapshot", "manifest":manifest}),
    ))
}

/// Read-only status of the configured Master updater/synchronizer.
pub async fn master_updater(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let client = online(&state, Region::parse(&region)?)?;
    Ok(Json(envelope(client.core.master_update_status().await)))
}
