use std::{
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use crate::{ranking::CachePolicy, region::Region};

/// Original Sirius configuration, parsed locally and used as an in-process library.
pub type SiriusConfig = sirius_api_proxy::config::Config;

#[derive(Clone)]
pub struct RegionConfig {
    pub region: Region,
    pub enabled: bool,
    pub client_version: Option<String>,
    pub base_url: Option<String>,
    pub master_dir: Option<PathBuf>,
    pub sirius: Option<SiriusConfig>,
}

#[derive(Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub api_key: Option<String>,
    pub ranking_cache: CachePolicy,
    pub regions: [RegionConfig; 2],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 8081)),
            api_key: None,
            ranking_cache: CachePolicy {
                fresh: Duration::from_secs(30),
                stale: Duration::from_secs(300),
                retry: Duration::from_secs(5),
            },
            regions: [
                RegionConfig {
                    region: Region::Global,
                    enabled: true,
                    client_version: None,
                    base_url: None,
                    master_dir: None,
                    sirius: None,
                },
                RegionConfig {
                    region: Region::Jp,
                    enabled: true,
                    client_version: Some("1.0.2".into()),
                    base_url: None,
                    master_dir: Some(PathBuf::from("artifacts/jp/master-decrypted")),
                    sirius: None,
                },
            ],
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        dotenvy::from_path(".env.local").ok();
        dotenvy::from_path(".env").ok();
        let mut config = Self {
            listen: SocketAddr::new(
                parse::<IpAddr>("HOST", "127.0.0.1")?,
                parse("PORT", "8081")?,
            ),
            api_key: optional("API_KEY"),
            ..Self::default()
        };
        config.ranking_cache = CachePolicy {
            fresh: Duration::from_secs(parse("RANKING_CACHE_TTL_SECS", "30")?),
            stale: Duration::from_secs(parse("RANKING_STALE_TTL_SECS", "300")?),
            retry: Duration::from_secs(parse("RANKING_RETRY_DELAY_SECS", "5")?),
        };
        if config.ranking_cache.stale < config.ranking_cache.fresh {
            return Err("RANKING_STALE_TTL_SECS must be at least RANKING_CACHE_TTL_SECS".into());
        }
        for (region, prefix) in config
            .regions
            .iter_mut()
            .zip(["OURNOTES_GLOBAL", "OURNOTES_JP"])
        {
            region.enabled = parse(
                &format!("{prefix}_ENABLED"),
                if region.enabled { "true" } else { "false" },
            )?;
            region.client_version = optional(&format!("{prefix}_CLIENT_VERSION"))
                .or_else(|| region.client_version.clone());
            region.base_url = optional(&format!("{prefix}_BASE_URL"));
            region.master_dir = optional(&format!("{prefix}_MASTER_DIR"))
                .map(PathBuf::from)
                .or_else(|| region.master_dir.clone());
            if let Some(path) = optional(&format!("{prefix}_PROTOCOL_CONFIG")) {
                if region.region != Region::Jp {
                    return Err("in-process Sirius integration currently supports jp only".into());
                }
                if region.enabled {
                    let bytes =
                        std::fs::read(path).map_err(|_| "protocol configuration unavailable")?;
                    if bytes.len() > 65536 {
                        return Err("protocol configuration too large".into());
                    }
                    let protocol: SiriusConfig = yaml_serde::from_slice(&bytes)
                        .map_err(|_| "invalid protocol configuration")?;
                    if protocol.region != sirius_api_proxy::region::Region::Jp {
                        return Err("JP protocol configuration must select jp".into());
                    }
                    if protocol.master_database.is_some()
                        || protocol.master_git.is_some()
                        || protocol.master_notify.is_some()
                        || protocol.asset_dispatch.is_some()
                        || protocol.tls.is_some()
                        || protocol.access_log.is_some()
                        || protocol.logging.is_some()
                        || protocol.client_auth.is_some()
                        || protocol.peer_token_env.is_some()
                    {
                        return Err("embedded configuration supports game queries, account pools, caches, outbound nodes, Master update and sync; standalone server administration settings are not supported".into());
                    }
                    protocol
                        .validate()
                        .map_err(|_| "protocol configuration validation failed")?;
                    region.client_version = Some(protocol.client_version.clone());
                    region.base_url = Some(protocol.endpoint.clone());
                    region.sirius = Some(protocol);
                }
            }
        }
        Ok(config)
    }

    pub fn region(&self, region: Region) -> &RegionConfig {
        match region {
            Region::Global => &self.regions[0],
            Region::Jp => &self.regions[1],
        }
    }
}

fn optional(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn parse<T: FromStr>(name: &str, default: &str) -> Result<T, String> {
    optional(name)
        .unwrap_or_else(|| default.to_owned())
        .parse()
        .map_err(|_| format!("invalid {name}"))
}
