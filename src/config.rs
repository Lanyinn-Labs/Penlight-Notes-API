use std::{
    env,
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use crate::region::Region;

#[derive(Clone)]
pub struct RegionConfig {
    pub region: Region,
    pub enabled: bool,
    pub client_version: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub api_key: Option<String>,
    pub regions: [RegionConfig; 2],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 8081)),
            api_key: None,
            regions: [
                RegionConfig {
                    region: Region::Global,
                    enabled: true,
                    client_version: None,
                    base_url: None,
                },
                RegionConfig {
                    region: Region::Jp,
                    enabled: false,
                    client_version: None,
                    base_url: None,
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
        for (region, prefix) in config
            .regions
            .iter_mut()
            .zip(["OURNOTES_GLOBAL", "OURNOTES_JP"])
        {
            region.enabled = parse(
                &format!("{prefix}_ENABLED"),
                if region.enabled { "true" } else { "false" },
            )?;
            region.client_version = optional(&format!("{prefix}_CLIENT_VERSION"));
            region.base_url = optional(&format!("{prefix}_BASE_URL"));
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
