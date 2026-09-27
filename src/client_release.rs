//! Runtime client configuration with a bundled fallback for offline startup.
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientRelease {
    pub client_version: String,
    pub endpoint: String,
    pub protocol_directory: std::path::PathBuf,
    pub cdn_root: String,
    pub cdn_username: String,
    pub cdn_password: String,
    pub master: MasterConstants,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasterConstants {
    pub key_hex: String,
    pub iv_hex: String,
}

impl ClientRelease {
    pub fn load() -> Result<Self, String> {
        match std::env::var("PENLIGHT_CLIENT_CONFIG") {
            Ok(path) if !path.trim().is_empty() => Self::read(std::path::Path::new(&path)),
            _ => match std::fs::metadata("data/jp-client.json") {
                Ok(_) => Self::read(std::path::Path::new("data/jp-client.json")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::bundled(),
                Err(_) => Err("cannot read JP client configuration".into()),
            },
        }
    }

    pub fn read(path: &std::path::Path) -> Result<Self, String> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|_| "cannot read JP client configuration")?;
        let mut document = String::new();
        file.take(65537)
            .read_to_string(&mut document)
            .map_err(|_| "cannot read JP client configuration")?;
        if document.len() > 65536 {
            return Err("JP client configuration exceeds 64 KiB".into());
        }
        Self::parse(&document)
    }

    pub fn bundled() -> Result<Self, String> {
        Self::parse(include_str!("../data/jp-client.json"))
    }

    fn parse(document: &str) -> Result<Self, String> {
        let release: Self =
            serde_json::from_str(document).map_err(|_| "invalid JP client configuration")?;
        let parts: Vec<_> = release.client_version.split('.').collect();
        if parts.len() != 3
            || parts
                .iter()
                .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
            || sirius_api_proxy::master::key_from_hex(&release.master.key_hex).is_err()
            || sirius_api_proxy::master::key_from_hex(&release.master.iv_hex).is_err()
            || release.cdn_username.trim().is_empty()
            || release.cdn_username.contains(':')
            || release.cdn_password.trim().is_empty()
        {
            return Err("invalid JP client version, CDN authentication or Master constants".into());
        }
        for origin in [&release.endpoint, &release.cdn_root] {
            let uri = origin
                .parse::<axum::http::Uri>()
                .map_err(|_| "invalid JP client service origin")?;
            if uri.scheme_str() != Some("https")
                || uri
                    .authority()
                    .is_none_or(|authority| authority.as_str().contains('@'))
                || uri.path() != "/"
                || uri.query().is_some()
            {
                return Err("JP client service origins must be HTTPS origins".into());
            }
        }
        Ok(release)
    }

    pub fn validate_protocol(&self) -> Result<(), String> {
        let bundle = sirius_api_proxy::protocol::ProtocolBundle::load(&self.protocol_directory)
            .map_err(|_| "client configuration requires unavailable or invalid protocol files; update the program/protocol first")?;
        if bundle.status.family != "jp" {
            return Err("JP client configuration requires JP protocol files".into());
        }
        Ok(())
    }

    pub fn environment_defaults(&self) -> [(&'static str, &str); 4] {
        [
            ("PENLIGHT_MASTER_KEY_HEX", &self.master.key_hex),
            ("PENLIGHT_MASTER_IV_HEX", &self.master.iv_hex),
            ("PENLIGHT_CDN_USERNAME", &self.cdn_username),
            ("PENLIGHT_CDN_PASSWORD", &self.cdn_password),
        ]
    }

    // Sirius takes client constants through environment references. Explicit
    // deployment overrides still win; otherwise use this build's reviewed bundle.
    pub fn install_defaults(&self) {
        for (name, value) in self.environment_defaults() {
            if std::env::var(name).ok().is_none_or(|v| v.trim().is_empty()) {
                std::env::set_var(name, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_rejects_invalid_constants_without_echoing_values() {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../data/jp-client.json")).unwrap();
        for field in ["key_hex", "iv_hex"] {
            let original = value["master"][field].clone();
            value["master"][field] = "private-sentinel".into();
            let error = ClientRelease::parse(&value.to_string()).err().unwrap();
            assert!(!error.contains("private-sentinel"));
            value["master"][field] = original;
        }
        value["cdn_username"] = "private:sentinel".into();
        let error = ClientRelease::parse(&value.to_string()).err().unwrap();
        assert!(!error.contains("private:sentinel"));
        value["cdn_username"] = "test-cdn".into();
        value["cdn_password"] = "".into();
        assert!(ClientRelease::parse(&value.to_string()).is_err());
        value["cdn_password"] = "test-password".into();
        value["client_version"] = "invalid".into();
        assert!(ClientRelease::parse(&value.to_string()).is_err());
    }
}
