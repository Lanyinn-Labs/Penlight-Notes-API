//! In-process integration of Sirius Project's MIT-licensed protocol implementation.
//! The unchanged upstream source and its original copyright notices live under
//! vendor/sirius-api-proxy. See docs/upstream-attribution.md for provenance.

use crate::{
    config::SiriusConfig,
    error::AppError,
    ranking::{RankingPoint, RankingRequest, RankingSource},
    region::Region,
};
use serde_json::{json, Value};
use sirius_api_proxy::{client::GameClient, peer::Operation};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub struct SiriusClient {
    pub core: Arc<GameClient>,
    settings: SiriusConfig,
    player_data: Mutex<Option<(Instant, Value)>>,
}

impl SiriusClient {
    pub fn new(settings: SiriusConfig) -> Result<Self, String> {
        if settings.region != sirius_api_proxy::region::Region::Jp {
            return Err("JP integration requires a JP protocol configuration".into());
        }
        let core = GameClient::new(settings.clone())
            .map_err(|error| format!("protocol initialization failed: {error}"))?;
        Ok(Self {
            core,
            settings,
            player_data: Mutex::new(None),
        })
    }

    pub async fn ready(&self) -> bool {
        self.upstream_status(Duration::from_secs(300)).await["status"] == "available"
    }

    pub async fn upstream_status(&self, ttl: Duration) -> Value {
        let observation = self.core.observation().await;
        upstream_snapshot(&observation, chrono::Utc::now(), ttl)
    }

    pub async fn application(&self) -> Result<Value, AppError> {
        let execution = self.core.public_query(Operation::Version {}).await;
        let available = match execution.result {
            Ok(_) => true,
            Err(sirius_api_proxy::error::AppError::Grpc(_)) => false,
            Err(error) => return Err(map_error(error)),
        };
        Ok(
            json!({"region":"jp", "status":if available {"available"} else {"unavailable"},
            "platform":self.core.platform(), "observation":execution.observation,
            "supported_rpcs":self.core.supported_routes()}),
        )
    }

    pub async fn query(&self, operation: Operation) -> Result<Value, AppError> {
        let ranking = matches!(
            operation,
            Operation::MusicRanking { .. } | Operation::ChallengeRanking { .. }
        );
        let mut value = self.core.public_call(operation).await.map_err(map_error)?;
        if ranking {
            if let Some(object) = value.as_object_mut() {
                object.remove("myRank");
                object.remove("myScore");
            }
        }
        Ok(value)
    }

    pub async fn whoami(&self) -> Result<Value, AppError> {
        self.core
            .call("/app.player.PlayerService/Whoami", json!({}))
            .await
            .map_err(map_error)
    }

    pub async fn account_data(&self) -> Result<Value, AppError> {
        let mut cache = self.player_data.lock().await;
        if let Some((at, data)) = cache.as_ref() {
            if at.elapsed() < Duration::from_secs(15) {
                return Ok(data.clone());
            }
        }
        let data = self
            .core
            .call("/app.player.PlayerService/GetPlayerData", json!({}))
            .await
            .map_err(map_error)?;
        if !data["playerData"].is_object() {
            return Err(AppError::UpstreamInvalidResponse);
        }
        *cache = Some((Instant::now(), data.clone()));
        Ok(data)
    }

    pub fn master_configured(&self) -> bool {
        self.core.master_directory().is_some()
    }

    pub async fn master_manifest(
        &self,
    ) -> Result<sirius_api_proxy::master_registry::PublishedManifest, AppError> {
        let root = self
            .core
            .master_directory()
            .ok_or(AppError::MasterDataUnavailable)?
            .to_path_buf();
        let scope = sirius_api_proxy::master_registry::Scope {
            region: self.core.region(),
            environment: self.core.environment().into(),
            platform: self.core.platform(),
        };
        tokio::task::spawn_blocking(move || {
            let document = sirius_api_proxy::master_registry::manifest(&root, None, scope.clone())
                .map_err(|_| AppError::MasterDataUnavailable)?;
            let manifest: sirius_api_proxy::master_registry::PublishedManifest =
                serde_json::from_slice(&document.bytes)
                    .map_err(|_| AppError::MasterDataUnavailable)?;
            manifest
                .validate(&scope)
                .map_err(|_| AppError::MasterDataUnavailable)?;
            Ok(manifest)
        })
        .await
        .map_err(|_| AppError::MasterDataUnavailable)?
    }

    pub async fn master_records_at(
        &self,
        manifest: &sirius_api_proxy::master_registry::PublishedManifest,
        table: &str,
    ) -> Result<Value, AppError> {
        let file = manifest
            .files
            .iter()
            .find(|file| file.name == format!("{table}.json"))
            .ok_or(AppError::NotFound)?;
        let root = self
            .core
            .master_directory()
            .ok_or(AppError::MasterDataUnavailable)?
            .to_path_buf();
        let snapshot = manifest.snapshot.clone();
        let hash = file.sha256.clone();
        let table = table.to_owned();
        let bytes = tokio::task::spawn_blocking(move || {
            sirius_api_proxy::master_registry::table(
                &root,
                sirius_api_proxy::region::Region::Jp,
                &snapshot,
                &table,
                &hash,
            )
            .map(|doc| doc.bytes)
        })
        .await
        .map_err(|_| AppError::MasterDataUnavailable)?
        .map_err(|_| AppError::MasterDataUnavailable)?;
        let mut document: Value =
            serde_json::from_slice(&bytes).map_err(|_| AppError::MasterDataUnavailable)?;
        let entries = document
            .as_object_mut()
            .and_then(|map| map.remove("_allData"))
            .filter(Value::is_array)
            .ok_or(AppError::MasterDataUnavailable)?;
        Ok(
            json!({"region":"jp", "source":"master_snapshot", "master_version":manifest.version, "snapshot":manifest.snapshot, "resource_version":manifest.resource_version, "entries":entries}),
        )
    }

    pub async fn master_records(&self, table: &str) -> Result<Value, AppError> {
        self.master_records_at(&self.master_manifest().await?, table)
            .await
    }

    pub async fn catalog(&self, resource: &str) -> Result<Value, AppError> {
        let spec = crate::offline_master::catalog_spec(resource).ok_or(AppError::NotFound)?;
        let manifest = self.master_manifest().await?;
        let document = self.master_records_at(&manifest, spec.table).await?;
        let texts = if spec.needs_text(&document) {
            Some(self.master_records_at(&manifest, "MasterText").await?)
        } else {
            None
        };
        crate::offline_master::normalize_jp_catalog(document, texts.as_ref(), resource)
    }

    pub async fn profile_summary(&self, raw: &Value) -> Value {
        // Pin one validated snapshot across all lookups. Public data remains usable
        // when optional enrichment tables are unavailable.
        let manifest = self.master_manifest().await.ok();
        let (ranks, cards, texts) = match &manifest {
            Some(manifest) => tokio::join!(
                self.master_records_at(manifest, "MasterPlayerRank"),
                self.master_records_at(manifest, "MasterMemberCard"),
                self.master_records_at(manifest, "MasterText"),
            ),
            None => return super::profile::summarize(raw, None, None, None, None),
        };
        super::profile::summarize(
            raw,
            ranks.ok().as_ref(),
            cards.ok().as_ref(),
            texts.ok().as_ref(),
            manifest.as_ref().map(|manifest| manifest.version.as_str()),
        )
    }

    pub fn start_worker(&self) -> Result<Option<BackgroundTask>, String> {
        let (shutdown, receiver) = tokio::sync::watch::channel(false);
        // Configuration allows exactly one Master update mode.
        let handle = if self.settings.master_update.is_some() {
            let worker = sirius_api_proxy::master_update::MasterUpdater::new(
                &self.settings,
                self.core.clone(),
            )
            .map_err(|_| "Master updater initialization failed")?;
            tokio::spawn(worker.run(receiver))
        } else if self.settings.master_sync.is_some() {
            let worker =
                sirius_api_proxy::master_sync::Syncer::new(&self.settings, self.core.clone())
                    .map_err(|_| "Master synchronization initialization failed")?;
            tokio::spawn(worker.run(receiver))
        } else {
            return Ok(None);
        };
        Ok(Some(BackgroundTask { shutdown, handle }))
    }
}

pub struct BackgroundTask {
    shutdown: tokio::sync::watch::Sender<bool>,
    handle: tokio::task::JoinHandle<()>,
}

impl BackgroundTask {
    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if tokio::time::timeout(Duration::from_secs(5), &mut self.handle)
            .await
            .is_err()
        {
            self.handle.abort();
        }
    }
}

impl Drop for BackgroundTask {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.handle.abort();
    }
}

pub fn map_error(error: sirius_api_proxy::error::AppError) -> AppError {
    use sirius_api_proxy::error::AppError as Upstream;
    match error {
        Upstream::AccountUnavailable
        | Upstream::PeerAccountUnavailable
        | Upstream::Grpc(7 | 16) => AppError::UpstreamAuthenticationUnavailable,
        Upstream::Grpc(8) => AppError::UpstreamRateLimited,
        Upstream::Grpc(5) | Upstream::NotFound => AppError::NotFound,
        Upstream::Grpc(14) | Upstream::Transport | Upstream::Proxy | Upstream::NodeUnavailable => {
            AppError::UpstreamUnavailable
        }
        Upstream::Grpc(status) => AppError::UpstreamGameError(status),
        Upstream::Timeout => AppError::UpstreamTimeout,
        Upstream::UnsupportedRegionOperation => AppError::ProtocolPending,
        Upstream::InvalidRequest => AppError::InvalidQuery,
        Upstream::MasterUnavailable => AppError::MasterDataUnavailable,
        _ => AppError::UpstreamInvalidResponse,
    }
}

pub struct SiriusRankingSource(pub Option<Arc<SiriusClient>>);

impl RankingSource for SiriusRankingSource {
    fn fetch<'a>(
        &'a self,
        region: Region,
        request: &'a RankingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<RankingPoint>, AppError>> + Send + 'a>> {
        Box::pin(async move {
            if region != Region::Jp {
                return Err(AppError::ProtocolPending);
            }
            let client = self.0.as_ref().ok_or(AppError::ProtocolPending)?;
            let data = client
                .query(Operation::EventRanking {
                    event_id: request.event_id,
                    ranks: request.ranks.clone(),
                })
                .await?;
            let entries = match data.get("ranking") {
                None => return Ok(Vec::new()), // Protobuf JSON can omit an empty repeated field.
                Some(value) => value.as_array().ok_or(AppError::UpstreamInvalidResponse)?,
            };
            entries
                .iter()
                .map(|entry| {
                    Ok(RankingPoint {
                        rank: entry["rank"]
                            .as_i64()
                            .and_then(|n| i32::try_from(n).ok())
                            .ok_or(AppError::UpstreamInvalidResponse)?,
                        point: match entry.get("point") {
                            None => 0,
                            Some(value) => value
                                .as_i64()
                                .and_then(|n| i32::try_from(n).ok())
                                .ok_or(AppError::UpstreamInvalidResponse)?,
                        },
                    })
                })
                .collect()
        })
    }
}

pub fn envelope(data: Value) -> Value {
    json!({"region":"jp", "source":"official_game_service", "protocol_implementation":"sirius_api_proxy", "data":data})
}

fn upstream_snapshot(
    observation: &sirius_api_proxy::client::Observation,
    now: chrono::DateTime<chrono::Utc>,
    ttl: Duration,
) -> Value {
    let age_ms = observation
        .observed_at
        .map(|at| (now - at).num_milliseconds().max(0) as u64);
    let status = match age_ms {
        None => "unknown",
        Some(age) if u128::from(age) >= ttl.as_millis() => "stale",
        _ if observation.grpc_status == Some(0) && !observation.maintenance => "available",
        _ => "unavailable",
    };
    json!({"configured":true, "status":status, "last_observed_at":observation.observed_at,
        "age_ms":age_ms, "grpc_status":observation.grpc_status, "maintenance":observation.maintenance})
}

#[cfg(test)]
mod health_tests {
    use super::*;
    #[test]
    fn health_distinguishes_unknown_success_failure_maintenance_and_staleness() {
        let now = chrono::Utc::now();
        let ttl = Duration::from_secs(300);
        let mut observation = sirius_api_proxy::client::Observation::default();
        assert_eq!(
            upstream_snapshot(&observation, now, ttl)["status"],
            "unknown"
        );
        observation.observed_at = Some(now);
        observation.grpc_status = Some(0);
        assert_eq!(
            upstream_snapshot(&observation, now, ttl)["status"],
            "available"
        );
        observation.maintenance = true;
        assert_eq!(
            upstream_snapshot(&observation, now, ttl)["status"],
            "unavailable"
        );
        observation.maintenance = false;
        observation.grpc_status = Some(16);
        assert_eq!(
            upstream_snapshot(&observation, now, ttl)["status"],
            "unavailable"
        );
        observation.grpc_status = Some(0);
        assert_eq!(
            upstream_snapshot(&observation, now + chrono::Duration::seconds(301), ttl)["status"],
            "stale"
        );
    }
}
