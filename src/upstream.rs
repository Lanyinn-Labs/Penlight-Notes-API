//! Single source of upstream version identity for API responses.
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
pub struct Provenance {
    pub version: String,
    pub revision: String,
}

pub fn provenance() -> &'static Provenance {
    static PROVENANCE: OnceLock<Provenance> = OnceLock::new();
    PROVENANCE.get_or_init(|| {
        serde_json::from_str(include_str!("../vendor/sirius-api-proxy/UPSTREAM.json"))
            .expect("verified upstream provenance must be valid")
    })
}
