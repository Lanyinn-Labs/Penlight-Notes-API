//! Activity routes, verified Master capabilities and cached cutoff responses.
use super::{
    handlers::{online, positive},
    SharedState,
};
use crate::{
    client::sirius::envelope, error::AppError, offline_master, ranking::RankingRequest,
    region::Region,
};
use axum::{
    extract::{rejection::QueryRejection, Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sirius_api_proxy::peer::Operation;

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
    let client = online(&state, region)?;
    ensure_event_ranking(&state, region, request.event_id).await?;
    client
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
    if !state.config.region(region).enabled {
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
