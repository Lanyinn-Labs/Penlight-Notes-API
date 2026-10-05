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
    ranking::RankingRequest,
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

pub async fn version() -> Json<Value> {
    Json(
        json!({"name": env!("CARGO_PKG_NAME"), "version": env!("CARGO_PKG_VERSION"), "stage": "online", "protocol_implementation":"sirius-api-proxy", "protocol_version":"1.3.3", "protocol_revision":"cd19e2fe1d7f6c9a69304e6718e95fbc61817561"}),
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

fn online(state: &SharedState, region: Region) -> Result<&SiriusClient, AppError> {
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

fn positive(raw: &str) -> Result<i64, AppError> {
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

pub async fn event_rankings(
    State(state): State<SharedState>,
    Path((region, event)): Path<(String, String)>,
    query: Result<Query<CutoffQuery>, QueryRejection>,
) -> Result<Json<Value>, AppError> {
    let Query(query) = query.map_err(|_| AppError::InvalidRankingQuery)?;
    let request = RankingRequest::parse(
        &event,
        query
            .ranks
            .as_deref()
            .ok_or(AppError::InvalidRankingQuery)?,
    )?;
    let region = Region::parse(&region)?;
    online(&state, region)?;
    ensure_event_ranking(&state, region, request.event_id).await?;
    online(&state, region)?
        .query(Operation::EventRanking {
            event_id: request.event_id,
            ranks: request.ranks,
        })
        .await
        .map(envelope)
        .map(Json)
}

pub async fn event_deck(
    State(state): State<SharedState>,
    Path((region, event, player)): Path<(String, String, String)>,
) -> Result<Json<Value>, AppError> {
    online(&state, Region::parse(&region)?)?
        .query(Operation::EventDeck {
            event_id: positive(&event)?,
            player_id: player,
        })
        .await
        .map(envelope)
        .map(Json)
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutoffQuery {
    #[serde(default)]
    ranks: Option<String>,
}

fn cutoff_request(
    state: &SharedState,
    event: &str,
    ranks: Option<&str>,
) -> Result<RankingRequest, AppError> {
    match ranks {
        Some(ranks) => RankingRequest::parse(event, ranks),
        None => {
            let mut request = RankingRequest::parse(event, "1")?;
            request.ranks = state.config.ranking_default_ranks.clone();
            Ok(request)
        }
    }
}

async fn event_table(state: &SharedState, region: Region) -> Result<Value, AppError> {
    let client = online(state, region)?;
    if client.master_configured() {
        return client.master_records("MasterEvent").await;
    }
    let directory = state
        .config
        .region(region)
        .master_dir
        .as_deref()
        .ok_or(AppError::MasterDataUnavailable)?;
    offline_master::records(region, directory, "MasterEvent").await
}

async fn ensure_event_ranking(
    state: &SharedState,
    region: Region,
    event_id: i64,
) -> Result<(), AppError> {
    // Explicit-ID queries remain usable without a Master store. When configured,
    // verified Master capabilities prevent querying a disabled or unknown event.
    let configured = state
        .sirius
        .as_ref()
        .is_some_and(|client| client.master_configured())
        || state.config.region(region).master_dir.is_some();
    if region != Region::Jp || !configured {
        return Ok(());
    }
    let document = event_table(state, region).await?;
    let entries = document["entries"]
        .as_array()
        .ok_or(AppError::MasterDataUnavailable)?;
    let event = entries
        .iter()
        .find(|entry| entry["_id"].as_i64() == Some(event_id))
        .ok_or(AppError::NotFound)?;
    match event["_isRankingDisabled"].as_bool() {
        Some(false) => Ok(()),
        Some(true) => Err(AppError::EventRankingDisabled),
        None => Err(AppError::MasterDataUnavailable),
    }
}

async fn active_event(
    state: &SharedState,
    region: Region,
) -> Result<(crate::events::Event, Value), AppError> {
    let document = event_table(state, region).await?;
    let entries = document["entries"]
        .as_array()
        .ok_or(AppError::MasterDataUnavailable)?;
    Ok((
        crate::events::current(entries, chrono::Utc::now())?,
        document,
    ))
}

pub async fn current_event(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let (event, document) = active_event(&state, Region::parse(&region)?).await?;
    Ok(Json(json!({"region":"jp", "source":document["source"],
        "master_version":document["master_version"], "snapshot":document["snapshot"], "event":event})))
}

pub async fn current_event_cutoffs(
    State(state): State<SharedState>,
    Path(region): Path<String>,
    query: Result<Query<CutoffQuery>, QueryRejection>,
) -> Result<(axum::http::HeaderMap, Json<Value>), AppError> {
    let region = Region::parse(&region)?;
    let Query(query) = query.map_err(|_| AppError::InvalidRankingQuery)?;
    // Validate ranks before reading Master data or contacting the game.
    let mut request = cutoff_request(&state, "1", query.ranks.as_deref())?;
    let (event, _) = active_event(&state, region).await?;
    if !event.ranking_enabled {
        return Err(AppError::EventRankingDisabled);
    }
    request.event_id = event.id;
    cutoff_response(&state, region, request).await
}

async fn cutoff_response(
    state: &SharedState,
    region: Region,
    request: RankingRequest,
) -> Result<(axum::http::HeaderMap, Json<Value>), AppError> {
    let response = state.rankings.get(region, request).await?;
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    Ok((
        headers,
        Json(serde_json::to_value(response).expect("cutoff response is serializable")),
    ))
}

pub async fn event_cutoffs(
    State(state): State<SharedState>,
    Path((region, event_id)): Path<(String, String)>,
    query: Result<Query<CutoffQuery>, QueryRejection>,
) -> Result<(axum::http::HeaderMap, Json<Value>), AppError> {
    let region = Region::parse(&region)?;
    if !state.config.region(region).enabled {
        return Err(AppError::RegionDisabled);
    }
    let Query(query) = query.map_err(|_| AppError::InvalidRankingQuery)?;
    let request = cutoff_request(&state, &event_id, query.ranks.as_deref())?;
    ensure_event_ranking(&state, region, request.event_id).await?;
    cutoff_response(&state, region, request).await
}

/// Read-only status of the configured Master updater/synchronizer.
pub async fn master_updater(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let client = online(&state, Region::parse(&region)?)?;
    Ok(Json(envelope(client.core.master_update_status().await)))
}
