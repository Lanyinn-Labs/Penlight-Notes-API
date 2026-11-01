//! Gekisou Arena routes: current season, leagues overview, and ranking rewards.
use super::SharedState;
use crate::{
    arena::{self, ArenaLeague, ArenaRankingReward},
    error::AppError,
    offline_master,
    region::Region,
};
use axum::{
    extract::{rejection::QueryRejection, Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) async fn arena_table(
    state: &SharedState,
    region: Region,
    table: &str,
) -> Result<Value, AppError> {
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
        return client.master_records(table).await;
    }
    let directory = state
        .config
        .region(region)
        .master_dir
        .as_deref()
        .ok_or(AppError::MasterDataUnavailable)?;
    offline_master::records(region, directory, table).await
}

pub async fn current_arena(
    State(state): State<SharedState>,
    Path(region): Path<String>,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    let season_doc = arena_table(&state, region, "MasterArenaSeason").await?;
    let season_entries = season_doc["entries"]
        .as_array()
        .ok_or(AppError::MasterDataUnavailable)?;
    let season = arena::current_season(season_entries, chrono::Utc::now())?;

    let league_doc = arena_table(&state, region, "MasterArenaLeague").await?;
    let league_entries = league_doc["entries"]
        .as_array()
        .ok_or(AppError::MasterDataUnavailable)?;
    let leagues = league_entries
        .iter()
        .map(ArenaLeague::from_master)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(json!({
        "region": "jp",
        "source": season_doc["source"],
        "master_version": season_doc["master_version"],
        "snapshot": season_doc["snapshot"],
        "season": season,
        "leagues": leagues,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankingRewardsQuery {
    #[serde(default)]
    league_id: Option<i64>,
}

pub async fn ranking_rewards(
    State(state): State<SharedState>,
    Path(region): Path<String>,
    query: Result<Query<RankingRewardsQuery>, QueryRejection>,
) -> Result<Json<Value>, AppError> {
    let region = Region::parse(&region)?;
    let Query(query) = query.map_err(|_| AppError::InvalidQuery)?;
    let reward_doc = arena_table(&state, region, "MasterArenaRankingReward").await?;
    let reward_entries = reward_doc["entries"]
        .as_array()
        .ok_or(AppError::MasterDataUnavailable)?;
    let mut rewards = reward_entries
        .iter()
        .map(ArenaRankingReward::from_master)
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(target_league) = query.league_id {
        if target_league <= 0 {
            return Err(AppError::InvalidQuery);
        }
        rewards.retain(|r| r.league_id == target_league || r.league_id == 0);
    }
    Ok(Json(json!({
        "region": "jp",
        "source": reward_doc["source"],
        "master_version": reward_doc["master_version"],
        "snapshot": reward_doc["snapshot"],
        "rewards": rewards,
    })))
}
