use std::{
    collections::BTreeMap, env, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration,
};

use crate::{client_release::ClientRelease, ranking::CachePolicy, region::Region};

pub type SiriusConfig = sirius_api_proxy::config::Config;

#[derive(Clone)]
pub struct RegionConfig {
    pub region: Region,
    pub enabled: bool,
    pub master_dir: Option<PathBuf>,
    pub sirius: Option<SiriusConfig>,
}

#[derive(Clone, Copy)]
pub struct RequestLimits {
    pub max_concurrent: usize,
    pub per_second: u32,
    pub burst: u32,
    pub timeout: Duration,
}

#[derive(Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub api_key: Option<String>,
    pub ranking_cache: CachePolicy,
    pub request_limits: RequestLimits,
    pub upstream_status_ttl: Duration,
    pub regions: [RegionConfig; 2],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 8081)),
            api_key: None,
            request_limits: RequestLimits {
                max_concurrent: 16,
                per_second: 30,
                burst: 60,
                timeout: Duration::from_secs(30),
            },
            upstream_status_ttl: Duration::from_secs(300),
            ranking_cache: CachePolicy {
                fresh: Duration::from_secs(30),
                stale: Duration::from_secs(300),
                retry: Duration::from_secs(5),
            },
            regions: [
                RegionConfig {
                    region: Region::Global,
                    enabled: true,
                    master_dir: None,
                    sirius: None,
                },
                RegionConfig {
                    region: Region::Jp,
                    enabled: true,
                    master_dir: None,
                    sirius: None,
                },
            ],
        }
    }
}

// A single environment reader also allows deterministic tests without mutating process state.
struct Environment<F>(F);
impl<F: Fn(&str) -> Option<String>> Environment<F> {
    fn optional(&self, name: &str) -> Option<String> {
        (self.0)(name)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }
    fn parse<T: FromStr>(&self, name: &str, default: T) -> Result<T, String> {
        match self.optional(name) {
            Some(value) => value.parse().map_err(|_| format!("invalid {name}")),
            None => Ok(default),
        }
    }
    fn path(&self, name: &str) -> Option<PathBuf> {
        self.optional(name).map(PathBuf::from)
    }
    fn seconds(&self, name: &str, default: Duration) -> Result<Duration, String> {
        self.parse(name, default.as_secs()).map(Duration::from_secs)
    }
}

impl Config {
    /// Deployment settings come from .env; client parameters come from runtime JSON.
    pub fn from_env() -> Result<Self, String> {
        load_dotenv()?;
        let release = ClientRelease::load()?;
        release.install_defaults();
        Self::from_release(|name| env::var(name).ok(), release)
    }

    #[cfg(test)]
    fn from_values(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        Self::from_release(get, ClientRelease::bundled()?)
    }

    fn from_release(
        get: impl Fn(&str) -> Option<String>,
        release: ClientRelease,
    ) -> Result<Self, String> {
        let values = Environment(|name: &str| {
            get(name)
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    release
                        .environment_defaults()
                        .into_iter()
                        .find(|(key, _)| *key == name)
                        .map(|(_, value)| value.to_owned())
                })
        });
        let mut config = Self::default();
        config.listen = values.parse("PENLIGHT_LISTEN", config.listen)?;
        config.api_key = values.optional("PENLIGHT_API_KEY");
        let limits = &mut config.request_limits;
        limits.max_concurrent = values.parse("PENLIGHT_MAX_CONCURRENT", limits.max_concurrent)?;
        limits.per_second = values.parse("PENLIGHT_REQUESTS_PER_SECOND", limits.per_second)?;
        limits.burst = values.parse("PENLIGHT_REQUEST_BURST", limits.burst)?;
        limits.timeout = values.seconds("PENLIGHT_REQUEST_TIMEOUT_SECONDS", limits.timeout)?;
        config.upstream_status_ttl =
            values.seconds("PENLIGHT_STATUS_TTL_SECONDS", config.upstream_status_ttl)?;
        if !(1..=65536).contains(&limits.max_concurrent)
            || limits.per_second == 0
            || limits.burst == 0
            || limits.timeout.is_zero()
            || config.upstream_status_ttl.is_zero()
        {
            return Err(
                "request limits and status TTL must be positive; concurrency must not exceed 65536"
                    .into(),
            );
        }
        let cache = &mut config.ranking_cache;
        cache.fresh = values.seconds("PENLIGHT_RANKING_FRESH_SECONDS", cache.fresh)?;
        cache.stale = values.seconds("PENLIGHT_RANKING_STALE_SECONDS", cache.stale)?;
        cache.retry = values.seconds("PENLIGHT_RANKING_RETRY_SECONDS", cache.retry)?;
        if cache.fresh.is_zero() || cache.retry.is_zero() || cache.stale < cache.fresh {
            return Err(
                "ranking cache durations must be positive; stale must be at least fresh".into(),
            );
        }
        config.regions[0].enabled = values.parse("PENLIGHT_GLOBAL_ENABLED", true)?;
        config.regions[0].master_dir = values.path("PENLIGHT_GLOBAL_SNAPSHOT_DIR");
        config.regions[1].enabled = values.parse("PENLIGHT_JP_ENABLED", true)?;
        config.regions[1].master_dir = values.path("PENLIGHT_JP_SNAPSHOT_DIR");
        config.regions[1].sirius =
            jp_protocol(&values, config.regions[1].master_dir.is_some(), &release)?;
        if config.regions[1].enabled
            && config.regions[1]
                .sirius
                .as_ref()
                .is_some_and(|p| p.master_update.is_some())
        {
            validate_master_secrets(&values)?;
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

pub fn load_dotenv() -> Result<(), String> {
    match dotenvy::from_path(".env") {
        Ok(()) => {}
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("invalid or unreadable .env".into()),
    }
    Ok(())
}

fn validate_master_secrets<F: Fn(&str) -> Option<String>>(
    values: &Environment<F>,
) -> Result<(), String> {
    for name in [
        "PENLIGHT_CDN_USERNAME",
        "PENLIGHT_CDN_PASSWORD",
        "PENLIGHT_MASTER_KEY_HEX",
        "PENLIGHT_MASTER_IV_HEX",
    ] {
        let value = values
            .optional(name)
            .ok_or_else(|| format!("Master download requires {name}"))?;
        if name == "PENLIGHT_CDN_USERNAME" && value.contains(':') {
            return Err("invalid PENLIGHT_CDN_USERNAME".into());
        }
        if name.starts_with("PENLIGHT_MASTER_")
            && sirius_api_proxy::master::key_from_hex(&value).is_err()
        {
            return Err(format!("invalid {name}: expected 32 bytes of hexadecimal"));
        }
    }
    Ok(())
}

fn jp_protocol<F: Fn(&str) -> Option<String>>(
    values: &Environment<F>,
    snapshot: bool,
    release: &ClientRelease,
) -> Result<Option<SiriusConfig>, String> {
    let mode = values
        .optional("PENLIGHT_JP_MASTER_MODE")
        .unwrap_or_else(|| "local".into());
    if !matches!(mode.as_str(), "local" | "download" | "sync") {
        return Err("PENLIGHT_JP_MASTER_MODE must be local, download or sync".into());
    }
    let accounts = values.optional("PENLIGHT_JP_ACCOUNTS");
    let online = values.parse("PENLIGHT_JP_ONLINE", accounts.is_some() || mode != "local")?;
    if !online {
        if mode != "local" {
            return Err("Master download/sync requires PENLIGHT_JP_ONLINE=true".into());
        }
        return Ok(None);
    }
    let accounts = accounts
        .map(|list| {
            list.split(',')
                .map(|path| {
                    let (explicit_name, path) = match path.trim().split_once('=') {
                        Some((name, path)) => (Some(name.trim()), path.trim()),
                        None => (None, path.trim()),
                    };
                    let path = PathBuf::from(path);
                    let name = explicit_name
                        .or_else(|| path.file_stem().and_then(|s| s.to_str()))
                        .filter(|name| !name.is_empty())
                        .ok_or("PENLIGHT_JP_ACCOUNTS contains an invalid name or path")?
                        .to_owned();
                    if path.as_os_str().is_empty() {
                        return Err("PENLIGHT_JP_ACCOUNTS contains an empty path".into());
                    }
                    Ok(sirius_api_proxy::accounts::AccountConfig {
                        name,
                        credentials_file: Some(path),
                        player_id_env: None,
                        credential_env: None,
                        global_identity_file: None,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();
    let master_directory = values.path("PENLIGHT_JP_MASTER_DIR");
    if snapshot && (master_directory.is_some() || mode != "local") {
        return Err("configure only one JP Master source: PENLIGHT_JP_SNAPSHOT_DIR or PENLIGHT_JP_MASTER_DIR".into());
    }
    let master_directory = master_directory
        .or_else(|| (!snapshot).then(|| PathBuf::from("artifacts/jp/master-store")));
    let interval = values.parse("PENLIGHT_JP_MASTER_INTERVAL_SECONDS", 300u64)?;
    let mut upstream = sirius_api_proxy::config::UpstreamConfig::default();
    upstream.timeout_ms = values.parse("PENLIGHT_JP_TIMEOUT_MS", upstream.timeout_ms)?;
    upstream.max_inflight = values.parse("PENLIGHT_JP_MAX_INFLIGHT", upstream.max_inflight)?;
    upstream.proxy_url_env = values
        .optional("PENLIGHT_JP_PROXY_URL")
        .map(|_| "PENLIGHT_JP_PROXY_URL".into());
    upstream.proxy_authorization_env = values
        .optional("PENLIGHT_JP_PROXY_AUTH")
        .map(|_| "PENLIGHT_JP_PROXY_AUTH".into());
    let cache_ttl = values.parse("PENLIGHT_JP_CACHE_TTL_SECONDS", 0u64)?;
    let response_cache = if cache_ttl == 0 {
        Default::default()
    } else {
        sirius_api_proxy::response_cache::Config::Memory {
            ttl_ms: cache_ttl
                .checked_mul(1000)
                .ok_or("invalid PENLIGHT_JP_CACHE_TTL_SECONDS")?,
            stale_while_revalidate_ms: 0,
            route_ttl_ms: Default::default(),
            max_entries: 1024,
            max_bytes: 32 * 1024 * 1024,
            max_entry_bytes: 8 * 1024 * 1024,
        }
    };
    let node_routing = values.optional("PENLIGHT_JP_PEER_URL").map(|origin| {
        sirius_api_proxy::node_routing::Config {
            targets: vec![sirius_api_proxy::node_routing::TargetConfig {
                name: "peer".into(),
                origin,
                token_env: "PENLIGHT_JP_PEER_TOKEN".into(),
                priority: 10,
                regional_paths: true,
                allow_http: false,
            }],
            ..Default::default()
        }
    });
    if node_routing.is_some() && values.optional("PENLIGHT_JP_PEER_TOKEN").is_none() {
        return Err("peer routing requires PENLIGHT_JP_PEER_TOKEN".into());
    }
    let master_update = if mode == "download" {
        let mut network = sirius_api_proxy::master_update::Network::default();
        network.connect_timeout_ms = values.parse(
            "PENLIGHT_JP_MASTER_CONNECT_TIMEOUT_MS",
            network.connect_timeout_ms,
        )?;
        network.request_timeout_ms = values.parse(
            "PENLIGHT_JP_MASTER_REQUEST_TIMEOUT_MS",
            network.request_timeout_ms,
        )?;
        network.retry_delay_ms =
            values.parse("PENLIGHT_JP_MASTER_RETRY_DELAY_MS", network.retry_delay_ms)?;
        network.max_retry_delay_ms = values.parse(
            "PENLIGHT_JP_MASTER_MAX_RETRY_DELAY_MS",
            network.max_retry_delay_ms,
        )?;
        network.attempts = values.parse("PENLIGHT_JP_MASTER_ATTEMPTS", network.attempts)?;
        network.update_timeout_seconds = values.parse(
            "PENLIGHT_JP_MASTER_TIMEOUT_SECONDS",
            network.update_timeout_seconds,
        )?;
        network.proxy_url_env = upstream.proxy_url_env.clone();
        network.proxy_authorization_env = upstream.proxy_authorization_env.clone();
        Some(sirius_api_proxy::config::MasterUpdateConfig {
            network,
            interval_seconds: interval,
            cdn_authorization: sirius_api_proxy::config::CdnAuthorization::Basic,
            username_env: Some("PENLIGHT_CDN_USERNAME".into()),
            key_hex_env: "PENLIGHT_MASTER_KEY_HEX".into(),
            iv_hex_env: "PENLIGHT_MASTER_IV_HEX".into(),
        })
    } else {
        None
    };
    let master_sync = if mode == "sync" {
        Some(sirius_api_proxy::master_sync::Config {
            origin: values
                .optional("PENLIGHT_JP_MASTER_SYNC_ORIGIN")
                .ok_or("sync requires PENLIGHT_JP_MASTER_SYNC_ORIGIN")?,
            token_env: "PENLIGHT_JP_MASTER_SYNC_TOKEN".into(),
            regional_paths: true,
            allow_http: false,
            interval_seconds: interval,
            timeout_seconds: values.parse("PENLIGHT_JP_MASTER_TIMEOUT_SECONDS", 600)?,
            request_timeout_ms: 60000,
        })
    } else {
        None
    };
    let cdn = release.cdn_root.as_str();
    let config = SiriusConfig {
        region: sirius_api_proxy::region::Region::Jp,
        platform: Some(sirius_api_proxy::region::Platform::Android),
        environment: "release".into(),
        endpoint: values
            .optional("PENLIGHT_JP_ENDPOINT")
            .unwrap_or_else(|| release.endpoint.clone()),
        client_version: values
            .optional("PENLIGHT_JP_CLIENT_VERSION")
            .unwrap_or_else(|| release.client_version.clone()),
        protocol_directory: values
            .path("PENLIGHT_JP_PROTOCOL_DIR")
            .unwrap_or_else(|| release.protocol_directory.clone()),
        session_lock: values.parse("PENLIGHT_JP_SESSION_LOCK", true)?,
        accounts,
        account_pool: sirius_api_proxy::accounts::PoolPolicy {
            failure_threshold: values.parse("PENLIGHT_JP_ACCOUNT_FAILURE_THRESHOLD", 2)?,
            cooldown_seconds: values.parse("PENLIGHT_JP_ACCOUNT_COOLDOWN_SECONDS", 30)?,
        },
        upstream,
        response_cache,
        node_routing,
        master_directory,
        master_update,
        master_sync,
        default_cdn_root: cdn.into(),
        cdn_credential_env: BTreeMap::from([(cdn.into(), "PENLIGHT_CDN_PASSWORD".into())]),
        api_token_env: "PENLIGHT_UNUSED_API_TOKEN".into(),
        internal_token_env: "PENLIGHT_UNUSED_INTERNAL_TOKEN".into(),
        listen: None,
        tls: None,
        access_log: None,
        logging: None,
        client_auth: None,
        peer_token_env: None,
        asset_dispatch: None,
        master_database: None,
        master_git: None,
        master_notify: None,
        player_id_env: None,
        player_credential_env: None,
        global_login: None,
        resource_snapshot: None,
    };
    config.validate().map_err(|error| match error {
        sirius_api_proxy::error::AppError::Config(reason) => {
            format!("invalid JP configuration: {reason}")
        }
        _ => "invalid JP protocol, account pool or Master policy".into(),
    })?;
    if mode == "sync" && values.optional("PENLIGHT_JP_MASTER_SYNC_TOKEN").is_none() {
        return Err("sync requires PENLIGHT_JP_MASTER_SYNC_TOKEN".into());
    }
    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(pairs: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_values(|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).into())
        })
    }

    #[test]
    fn defaults_are_offline_and_old_variables_have_no_effect() {
        let config = load(&[
            ("HOST", "0.0.0.0"),
            ("PORT", "9000"),
            ("API_KEY", "obsolete"),
            ("OURNOTES_JP_PROTOCOL_CONFIG", "missing.json"),
            ("MASTER_AUTO_UPDATE", "true"),
            ("API_MAX_CONCURRENT_REQUESTS", "0"),
            ("PENLIGHT_CONFIG", "missing.json"),
        ])
        .unwrap();
        assert_eq!(config.listen, "127.0.0.1:8081".parse().unwrap());
        assert!(config.api_key.is_none());
        assert!(config.regions[1].sirius.is_none());
        assert_eq!(config.request_limits.max_concurrent, 16);
    }

    #[test]
    fn environment_configures_service_policy_directly() {
        let config = load(&[
            ("PENLIGHT_LISTEN", "[::1]:9001"),
            ("PENLIGHT_API_KEY", "key"),
            ("PENLIGHT_MAX_CONCURRENT", "8"),
            ("PENLIGHT_REQUEST_BURST", "100"),
            ("PENLIGHT_RANKING_FRESH_SECONDS", "60"),
            ("PENLIGHT_GLOBAL_ENABLED", "false"),
        ])
        .unwrap();
        assert_eq!(config.listen, "[::1]:9001".parse().unwrap());
        assert_eq!(config.api_key.as_deref(), Some("key"));
        assert_eq!(config.request_limits.max_concurrent, 8);
        assert_eq!(config.request_limits.burst, 100);
        assert_eq!(config.ranking_cache.fresh, Duration::from_secs(60));
        assert!(!config.regions[0].enabled);
    }

    #[test]
    fn invalid_policy_is_rejected_without_echoing_values() {
        for (name, value) in [
            ("PENLIGHT_LISTEN", "private-sentinel"),
            ("PENLIGHT_MAX_CONCURRENT", "0"),
            ("PENLIGHT_MAX_CONCURRENT", "65537"),
            ("PENLIGHT_REQUESTS_PER_SECOND", "0"),
            ("PENLIGHT_REQUEST_BURST", "0"),
            ("PENLIGHT_REQUEST_TIMEOUT_SECONDS", "0"),
            ("PENLIGHT_STATUS_TTL_SECONDS", "0"),
            ("PENLIGHT_RANKING_FRESH_SECONDS", "0"),
            ("PENLIGHT_RANKING_RETRY_SECONDS", "0"),
            ("PENLIGHT_RANKING_STALE_SECONDS", "1"),
            ("PENLIGHT_JP_MASTER_MODE", "invalid"),
            ("PENLIGHT_JP_ENABLED", "yes"),
        ] {
            let error = load(&[(name, value)]).err().unwrap();
            assert!(!error.contains("private-sentinel"));
        }
    }

    #[test]
    fn account_paths_enable_online_and_version_has_one_default() {
        let config = load(&[("PENLIGHT_JP_ACCOUNTS", "secrets/a.json, secrets/b.json")]).unwrap();
        let protocol = config.regions[1].sirius.as_ref().unwrap();
        assert_eq!(
            protocol.client_version,
            ClientRelease::bundled().unwrap().client_version
        );
        assert_eq!(protocol.accounts.len(), 2);
        assert_eq!(protocol.accounts[1].name, "b");
        assert_eq!(
            protocol.accounts[1].credentials_file.as_deref(),
            Some(std::path::Path::new("secrets/b.json"))
        );
        assert_eq!(
            protocol.master_directory.as_deref(),
            Some(std::path::Path::new("artifacts/jp/master-store"))
        );
        assert!(protocol.master_update.is_none());
        assert!(load(&[("PENLIGHT_JP_ACCOUNTS", "secrets/a.json,")]).is_err());
        assert!(load(&[("PENLIGHT_JP_ACCOUNTS", "a/account.json,b/account.json")]).is_err());
        assert!(load(&[
            ("PENLIGHT_JP_ACCOUNTS", "a.json"),
            ("PENLIGHT_JP_ONLINE", "false")
        ])
        .unwrap()
        .regions[1]
            .sirius
            .is_none());
    }

    #[test]
    fn anonymous_online_and_explicit_client_version_are_supported() {
        let config = load(&[
            ("PENLIGHT_JP_ONLINE", "true"),
            ("PENLIGHT_JP_CLIENT_VERSION", "1.0.3"),
        ])
        .unwrap();
        let protocol = config.regions[1].sirius.as_ref().unwrap();
        assert!(protocol.accounts.is_empty());
        assert_eq!(protocol.client_version, "1.0.3");
    }

    #[test]
    fn snapshot_and_store_are_mutually_exclusive() {
        let config = load(&[
            ("PENLIGHT_GLOBAL_SNAPSHOT_DIR", "global"),
            ("PENLIGHT_JP_SNAPSHOT_DIR", "jp"),
            ("PENLIGHT_JP_ONLINE", "true"),
        ])
        .unwrap();
        assert_eq!(config.regions[0].master_dir, Some(PathBuf::from("global")));
        assert!(config.regions[1]
            .sirius
            .as_ref()
            .unwrap()
            .master_directory
            .is_none());
        assert!(load(&[
            ("PENLIGHT_JP_SNAPSHOT_DIR", "jp"),
            ("PENLIGHT_JP_MASTER_DIR", "store"),
            ("PENLIGHT_JP_ONLINE", "true")
        ])
        .is_err());
    }

    #[test]
    fn download_uses_bundled_cdn_and_master_constants_and_checks_overrides() {
        let mut pairs = vec![
            ("PENLIGHT_JP_MASTER_MODE", "download"),
            ("PENLIGHT_JP_MASTER_INTERVAL_SECONDS", "600"),
        ];
        let config = load(&pairs).unwrap();
        assert_eq!(
            config.regions[1]
                .sirius
                .as_ref()
                .unwrap()
                .master_update
                .as_ref()
                .unwrap()
                .interval_seconds,
            600
        );
        for (name, value) in [
            ("PENLIGHT_MASTER_IV_HEX", "private-sentinel"),
            ("PENLIGHT_CDN_USERNAME", "private:sentinel"),
        ] {
            let mut invalid = pairs.clone();
            invalid.push((name, value));
            let error = load(&invalid).err().unwrap();
            assert!(error.contains(name));
            assert!(!error.contains(value));
        }
        pairs.push(("PENLIGHT_JP_ONLINE", "false"));
        assert!(load(&pairs).is_err());
    }

    #[test]
    fn sync_uses_a_single_mode_and_requires_an_owner_and_token() {
        assert!(load(&[("PENLIGHT_JP_MASTER_MODE", "sync")]).is_err());
        assert!(load(&[
            ("PENLIGHT_JP_MASTER_MODE", "sync"),
            ("PENLIGHT_JP_MASTER_SYNC_ORIGIN", "https://owner.example")
        ])
        .is_err());
        let config = load(&[
            ("PENLIGHT_JP_MASTER_MODE", "sync"),
            ("PENLIGHT_JP_MASTER_SYNC_ORIGIN", "https://owner.example"),
            ("PENLIGHT_JP_MASTER_SYNC_TOKEN", "private-token"),
        ])
        .unwrap();
        let protocol = config.regions[1].sirius.as_ref().unwrap();
        assert!(protocol.master_update.is_none());
        assert!(protocol.master_sync.is_some());
    }
    #[test]
    fn explicit_account_names_and_optional_query_features_are_supported() {
        let config = load(&[
            (
                "PENLIGHT_JP_ACCOUNTS",
                "existing=secrets/long.client.credentials.json",
            ),
            ("PENLIGHT_JP_CACHE_TTL_SECONDS", "15"),
            ("PENLIGHT_JP_PEER_URL", "https://peer.example"),
            ("PENLIGHT_JP_PEER_TOKEN", "peer-token"),
        ])
        .unwrap();
        let protocol = config.regions[1].sirius.as_ref().unwrap();
        assert_eq!(protocol.accounts[0].name, "existing");
        assert!(matches!(
            protocol.response_cache,
            sirius_api_proxy::response_cache::Config::Memory { ttl_ms: 15000, .. }
        ));
        assert_eq!(
            protocol.node_routing.as_ref().unwrap().targets[0].origin,
            "https://peer.example"
        );
        assert!(load(&[
            ("PENLIGHT_JP_ONLINE", "true"),
            ("PENLIGHT_JP_PEER_URL", "https://peer.example")
        ])
        .is_err());
    }
}
