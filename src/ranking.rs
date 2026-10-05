//! Activity cutoff queries with bounded caching and coalesced refreshes.

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

pub trait RankingSource: Send + Sync {
    fn fetch<'a>(
        &'a self,
        region: Region,
        request: &'a RankingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<RankingPoint>, AppError>> + Send + 'a>>;
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
    last_error: Option<AppError>,
}

pub struct RankingService {
    source: Arc<dyn RankingSource>,
    policy: CachePolicy,
    slots: Mutex<HashMap<CacheKey, Arc<AsyncMutex<SlotState>>>>,
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
    complete: bool,
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
                if let Some(evicted) = slots
                    .iter()
                    .find(|(_, slot)| Arc::strong_count(slot) == 1)
                    .map(|(key, _)| key.clone())
                {
                    slots.remove(&evicted);
                } else {
                    return Err(AppError::ApiBusy);
                }
            }
            slots.entry(key).or_default().clone()
        };

        let mut state = slot.lock().await;
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

        let result = self
            .source
            .fetch(region, &request)
            .await
            .and_then(|points| {
                validate_points(&request, &points)?;
                Ok(points)
            });
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

fn validate_points(request: &RankingRequest, points: &[RankingPoint]) -> Result<(), AppError> {
    let requested: HashSet<i32> = request.ranks.iter().copied().collect();
    let mut seen = HashSet::new();
    if points
        .iter()
        .any(|item| item.point < 0 || !requested.contains(&item.rank) || !seen.insert(item.rank))
    {
        return Err(AppError::UpstreamInvalidResponse);
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
        .clone()
        .unwrap_or(AppError::UpstreamUnavailable))
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
        complete: request.ranks.iter().all(|rank| points.contains_key(rank)),
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

#[cfg(test)]
mod tests {
    use super::*;
    struct EmptySource(std::sync::atomic::AtomicUsize);
    impl RankingSource for EmptySource {
        fn fetch<'a>(
            &'a self,
            _: Region,
            _: &'a RankingRequest,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<RankingPoint>, AppError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(vec![])
            })
        }
    }
    #[tokio::test]
    async fn cache_pressure_keeps_active_slots_and_remains_bounded() {
        let source = Arc::new(EmptySource(std::sync::atomic::AtomicUsize::new(0)));
        let service = RankingService::new(
            source.clone(),
            CachePolicy {
                fresh: Duration::from_secs(30),
                stale: Duration::from_secs(300),
                retry: Duration::from_secs(5),
            },
        );
        let mut pinned = vec![];
        {
            let mut slots = service.slots.lock().unwrap();
            for event_id in 1..=MAX_CACHE_KEYS as i64 {
                let slot = Arc::new(AsyncMutex::new(SlotState::default()));
                pinned.push(slot.clone());
                slots.insert(
                    CacheKey {
                        region: Region::Jp,
                        event_id,
                        ranks: vec![100],
                    },
                    slot,
                );
            }
        }
        let request = RankingRequest::parse("2048", "100").unwrap();
        assert!(matches!(
            service.get(Region::Jp, request.clone()).await,
            Err(AppError::ApiBusy)
        ));
        assert_eq!(source.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(service.slots.lock().unwrap().len(), MAX_CACHE_KEYS);
        drop(pinned);
        assert!(service.get(Region::Jp, request).await.is_ok());
        assert_eq!(service.slots.lock().unwrap().len(), MAX_CACHE_KEYS);
        assert_eq!(source.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
    #[test]
    fn malformed_points_never_replace_a_verified_snapshot() {
        let request = RankingRequest::parse("1", "100").unwrap();
        for points in [
            vec![RankingPoint {
                rank: 100,
                point: -1,
            }],
            vec![
                RankingPoint {
                    rank: 100,
                    point: 1,
                },
                RankingPoint {
                    rank: 100,
                    point: 2,
                },
            ],
            vec![RankingPoint {
                rank: 101,
                point: 1,
            }],
        ] {
            assert!(validate_points(&request, &points).is_err());
        }
        assert!(validate_points(
            &request,
            &[RankingPoint {
                rank: 100,
                point: 0
            }]
        )
        .is_ok());
        assert!(validate_points(&request, &[]).is_ok());
    }
}
