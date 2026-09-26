//! Activity cutoff queries and cache. The wire protocol is not yet validated.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tokio::sync::Mutex as AsyncMutex;

use crate::{error::AppError, region::Region};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankingRequest {
    pub event_id: i64,
    pub ranks: Vec<i32>,
}

impl RankingRequest {
    pub fn parse(event_id: &str, ranks: &str) -> Result<Self, AppError> {
        let event_id = event_id
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or(AppError::InvalidRankingQuery)?;
        let mut parsed = Vec::new();
        for rank in ranks.split(',') {
            let rank = rank
                .parse::<i32>()
                .ok()
                .filter(|rank| *rank > 0)
                .ok_or(AppError::InvalidRankingQuery)?;
            parsed.push(rank);
            if parsed.len() > 20 {
                return Err(AppError::InvalidRankingQuery);
            }
        }
        parsed.sort_unstable();
        parsed.dedup();
        if parsed.is_empty() {
            return Err(AppError::InvalidRankingQuery);
        }
        Ok(Self {
            event_id,
            ranks: parsed,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankingPoint {
    pub rank: i32,
    pub point: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    ProtocolPending,
    Unauthorized,
    RateLimited,
    Unavailable,
    InvalidResponse,
    NotFound,
    Timeout,
    GameError(u16),
}

impl SourceError {
    fn app_error(&self) -> AppError {
        match self {
            Self::ProtocolPending => AppError::ProtocolPending,
            Self::Unauthorized => AppError::UpstreamAuthenticationUnavailable,
            Self::RateLimited => AppError::UpstreamRateLimited,
            Self::Unavailable => AppError::UpstreamUnavailable,
            Self::InvalidResponse => AppError::UpstreamInvalidResponse,
            Self::NotFound => AppError::NotFound,
            Self::Timeout => AppError::UpstreamTimeout,
            Self::GameError(status) => AppError::UpstreamGameError(*status),
        }
    }
}

pub trait RankingSource: Send + Sync {
    fn fetch<'a>(
        &'a self,
        region: Region,
        request: &'a RankingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<RankingPoint>, SourceError>> + Send + 'a>>;
}

pub struct PendingRankingSource;

impl RankingSource for PendingRankingSource {
    fn fetch<'a>(
        &'a self,
        _region: Region,
        _request: &'a RankingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<RankingPoint>, SourceError>> + Send + 'a>> {
        Box::pin(async { Err(SourceError::ProtocolPending) })
    }
}

#[derive(Clone, Copy)]
pub struct CachePolicy {
    pub fresh: Duration,
    pub stale: Duration,
    pub retry: Duration,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct CacheKey {
    region: Region,
    event_id: i64,
    ranks: Vec<i32>,
}

struct Snapshot {
    points: Vec<RankingPoint>,
    observed_at_unix_ms: u128,
    fetched_at: Instant,
}

#[derive(Default)]
struct SlotState {
    snapshot: Option<Snapshot>,
    retry_after: Option<Instant>,
    last_error: Option<SourceError>,
}

#[derive(Default)]
struct CacheSlot {
    refresh: AsyncMutex<()>,
    state: AsyncMutex<SlotState>,
}

pub struct RankingService {
    source: Arc<dyn RankingSource>,
    policy: CachePolicy,
    slots: Mutex<HashMap<CacheKey, Arc<CacheSlot>>>,
}

const MAX_CACHE_KEYS: usize = 1024;

#[derive(Serialize)]
pub struct Cutoff {
    rank: i32,
    point: Option<i32>,
}

#[derive(Serialize)]
pub struct CutoffResponse {
    region: Region,
    event_id: i64,
    source: &'static str,
    status: &'static str,
    observed_at_unix_ms: u128,
    age_ms: u128,
    cutoffs: Vec<Cutoff>,
}

impl RankingService {
    pub fn new(source: Arc<dyn RankingSource>, policy: CachePolicy) -> Self {
        Self {
            source,
            policy,
            slots: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get(
        &self,
        region: Region,
        request: RankingRequest,
    ) -> Result<CutoffResponse, AppError> {
        let key = CacheKey {
            region,
            event_id: request.event_id,
            ranks: request.ranks.clone(),
        };
        let slot = {
            let mut slots = self.slots.lock().expect("ranking cache mutex poisoned");
            if !slots.contains_key(&key) && slots.len() >= MAX_CACHE_KEYS {
                if let Some(evicted) = slots.keys().next().cloned() {
                    slots.remove(&evicted);
                }
            }
            slots.entry(key).or_default().clone()
        };

        {
            let state = slot.state.lock().await;
            if let Some(snapshot) = &state.snapshot {
                if snapshot.fetched_at.elapsed() < self.policy.fresh {
                    return Ok(response(region, &request, snapshot, "fresh"));
                }
            }
        }

        let _refresh = slot.refresh.lock().await;
        {
            let state = slot.state.lock().await;
            if let Some(snapshot) = &state.snapshot {
                if snapshot.fetched_at.elapsed() < self.policy.fresh {
                    return Ok(response(region, &request, snapshot, "fresh"));
                }
            }
            if state
                .retry_after
                .is_some_and(|until| Instant::now() < until)
            {
                return cached_or_error(region, &request, &state, self.policy.stale);
            }
        }

        let result = self
            .source
            .fetch(region, &request)
            .await
            .and_then(|points| {
                validate_points(&request, &points)?;
                Ok(points)
            });
        let mut state = slot.state.lock().await;
        match result {
            Ok(points) => {
                let observed_at_unix_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock predates Unix epoch")
                    .as_millis();
                state.snapshot = Some(Snapshot {
                    points,
                    observed_at_unix_ms,
                    fetched_at: Instant::now(),
                });
                state.retry_after = None;
                state.last_error = None;
                Ok(response(
                    region,
                    &request,
                    state.snapshot.as_ref().expect("snapshot just written"),
                    "fresh",
                ))
            }
            Err(error) => {
                state.retry_after = Some(Instant::now() + self.policy.retry);
                state.last_error = Some(error);
                cached_or_error(region, &request, &state, self.policy.stale)
            }
        }
    }
}

fn validate_points(request: &RankingRequest, points: &[RankingPoint]) -> Result<(), SourceError> {
    let requested: HashSet<i32> = request.ranks.iter().copied().collect();
    let mut seen = HashSet::new();
    if points
        .iter()
        .any(|item| item.point < 0 || !requested.contains(&item.rank) || !seen.insert(item.rank))
    {
        return Err(SourceError::InvalidResponse);
    }
    Ok(())
}

fn cached_or_error(
    region: Region,
    request: &RankingRequest,
    state: &SlotState,
    stale_ttl: Duration,
) -> Result<CutoffResponse, AppError> {
    if let Some(snapshot) = &state.snapshot {
        if snapshot.fetched_at.elapsed() < stale_ttl {
            return Ok(response(region, request, snapshot, "stale"));
        }
    }
    Err(state
        .last_error
        .as_ref()
        .map_or(AppError::UpstreamUnavailable, SourceError::app_error))
}

fn response(
    region: Region,
    request: &RankingRequest,
    snapshot: &Snapshot,
    status: &'static str,
) -> CutoffResponse {
    let points: HashMap<i32, i32> = snapshot
        .points
        .iter()
        .map(|point| (point.rank, point.point))
        .collect();
    CutoffResponse {
        region,
        event_id: request.event_id,
        source: "official_game_service",
        status,
        observed_at_unix_ms: snapshot.observed_at_unix_ms,
        age_ms: snapshot.fetched_at.elapsed().as_millis(),
        cutoffs: request
            .ranks
            .iter()
            .map(|rank| Cutoff {
                rank: *rank,
                point: points.get(rank).copied(),
            })
            .collect(),
    }
}
