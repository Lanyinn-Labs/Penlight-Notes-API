//! In-process integration of Sirius Project's MIT-licensed protocol implementation.
//! The unchanged upstream source and its original copyright notices live under
//! vendor/sirius-api-proxy. See docs/upstream-attribution.md for provenance.

use crate::{
    config::SiriusConfig,
    error::AppError,
    ranking::{RankingPoint, RankingRequest, RankingSource, SourceError},
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

pub enum Query {
    System,
    Announcements(i32),
    Announcement(i64),
    Profile(i64),
    EventRanking(RankingRequest),
    EventDeck(i64, String),
    MusicRanking(i64),
    ChallengeRanking(i64),
    Account,
    PlayerData,
}

impl Query {
    fn operation(&self) -> Result<Operation, AppError> {
        let positive = |id: i64| {
            if id > 0 {
                Ok(id)
            } else {
                Err(AppError::InvalidQuery)
            }
        };
        Ok(match self {
            Self::System => Operation::Version {},
            Self::Announcements(tab) if (0..=2).contains(tab) => {
                Operation::Announcements { tab: *tab }
            }
            Self::Announcements(_) => return Err(AppError::InvalidQuery),
            Self::Announcement(id) => Operation::Announcement { id: positive(*id)? },
            Self::Profile(id) => Operation::Profile {
                profile_id: positive(*id)?,
            },
            Self::EventRanking(request) => {
                let ranks = request
                    .ranks
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                let checked = RankingRequest::parse(&request.event_id.to_string(), &ranks)?;
                Operation::EventRanking {
                    event_id: checked.event_id,
                    ranks: checked.ranks,
                }
            }
            Self::EventDeck(id, player) => {
                if player.is_empty()
                    || player.len() > 128
                    || !player
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                {
                    return Err(AppError::InvalidQuery);
                }
                Operation::EventDeck {
                    event_id: positive(*id)?,
                    player_id: player.clone(),
                }
            }
            Self::MusicRanking(id) => Operation::MusicRanking {
                music_id: positive(*id)?,
            },
            Self::ChallengeRanking(id) => Operation::ChallengeRanking {
                challenge_music_id: positive(*id)?,
            },
            Self::Account | Self::PlayerData => return Err(AppError::InvalidQuery),
        })
    }
}

pub struct SiriusClient {
    pub core: Arc<GameClient>,
    settings: SiriusConfig,
    observation: std::sync::Mutex<Option<(Instant, bool)>>,
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
            observation: std::sync::Mutex::new(None),
            player_data: Mutex::new(None),
        })
    }

    pub fn ready(&self) -> bool {
        self.observation
            .lock()
            .expect("observation mutex poisoned")
            .as_ref()
            .is_some_and(|(at, ready)| *ready && at.elapsed() < Duration::from_secs(60))
    }

    pub async fn query(&self, query: Query) -> Result<Value, AppError> {
        if matches!(query, Query::System) {
            let execution = self.core.public_query(Operation::Version {}).await;
            let available = match execution.result {
                Ok(_) => true,
                Err(sirius_api_proxy::error::AppError::Grpc(_)) => false,
                Err(error) => return Err(map_error(error)),
            };
            *self.observation.lock().expect("observation mutex poisoned") =
                Some((Instant::now(), available));
            return Ok(
                json!({"region":"jp", "status":if available {"available"} else {"unavailable"}, "platform":self.core.platform(), "observation":execution.observation, "supported_rpcs":self.core.supported_routes()}),
            );
        }
        let mut value = match query {
            Query::Account => {
                self.core
                    .call("/app.player.PlayerService/Whoami", json!({}))
                    .await
            }
            Query::PlayerData => {
                self.core
                    .call("/app.player.PlayerService/GetPlayerData", json!({}))
                    .await
            }
            _ => self.core.public_call(query.operation()?).await,
        }
        .map_err(map_error)?;
        if matches!(query, Query::MusicRanking(_) | Query::ChallengeRanking(_)) {
            if let Some(object) = value.as_object_mut() {
                object.remove("myRank");
                object.remove("myScore");
            }
        }
        Ok(value)
    }

    pub async fn account_data(&self) -> Result<Value, AppError> {
        let mut cache = self.player_data.lock().await;
        if let Some((at, data)) = cache.as_ref() {
            if at.elapsed() < Duration::from_secs(15) {
                return Ok(data.clone());
            }
        }
        let data = self.query(Query::PlayerData).await?;
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
        let texts = self.master_records_at(&manifest, "MasterText").await?;
        crate::offline_master::normalize_jp_catalog(document, Some(&texts), resource)
    }

    pub fn start_workers(&self) -> Result<BackgroundTasks, String> {
        let (shutdown, receiver) = tokio::sync::watch::channel(false);
        let mut handles = Vec::new();
        if self.settings.master_update.is_some() {
            let worker = sirius_api_proxy::master_update::MasterUpdater::new(
                &self.settings,
                self.core.clone(),
            )
            .map_err(|_| "Master updater initialization failed")?;
            handles.push(tokio::spawn(worker.run(receiver.clone())));
        }
        if self.settings.master_sync.is_some() {
            let worker =
                sirius_api_proxy::master_sync::Syncer::new(&self.settings, self.core.clone())
                    .map_err(|_| "Master synchronization initialization failed")?;
            handles.push(tokio::spawn(worker.run(receiver)));
        }
        Ok(BackgroundTasks { shutdown, handles })
    }
}

pub struct BackgroundTasks {
    shutdown: tokio::sync::watch::Sender<bool>,
    handles: Vec<tokio::task::JoinHandle<()>>,
}

impl BackgroundTasks {
    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        for mut handle in self.handles.drain(..) {
            if tokio::time::timeout(Duration::from_secs(5), &mut handle)
                .await
                .is_err()
            {
                handle.abort();
            }
        }
    }
}

impl Drop for BackgroundTasks {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        for handle in &self.handles {
            handle.abort();
        }
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
    ) -> Pin<Box<dyn Future<Output = Result<Vec<RankingPoint>, SourceError>> + Send + 'a>> {
        Box::pin(async move {
            if region != Region::Jp {
                return Err(SourceError::ProtocolPending);
            }
            let client = self.0.as_ref().ok_or(SourceError::ProtocolPending)?;
            let data = client
                .query(Query::EventRanking(request.clone()))
                .await
                .map_err(|error| match error {
                    AppError::ProtocolPending => SourceError::ProtocolPending,
                    AppError::UpstreamAuthenticationUnavailable => SourceError::Unauthorized,
                    AppError::UpstreamRateLimited => SourceError::RateLimited,
                    AppError::UpstreamTimeout => SourceError::Timeout,
                    AppError::NotFound => SourceError::NotFound,
                    AppError::UpstreamGameError(status) => SourceError::GameError(status),
                    AppError::UpstreamInvalidResponse => SourceError::InvalidResponse,
                    _ => SourceError::Unavailable,
                })?;
            let entries = match data.get("ranking") {
                None => return Ok(Vec::new()), // Protobuf JSON can omit an empty repeated field.
                Some(value) => value.as_array().ok_or(SourceError::InvalidResponse)?,
            };
            entries
                .iter()
                .map(|entry| {
                    Ok(RankingPoint {
                        rank: entry["rank"]
                            .as_i64()
                            .and_then(|n| i32::try_from(n).ok())
                            .ok_or(SourceError::InvalidResponse)?,
                        point: match entry.get("point") {
                            None => 0,
                            Some(value) => value
                                .as_i64()
                                .and_then(|n| i32::try_from(n).ok())
                                .ok_or(SourceError::InvalidResponse)?,
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
