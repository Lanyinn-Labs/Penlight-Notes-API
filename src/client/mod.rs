//! Game service client and region-specific protocol integration.
//! Upstream requests are unavailable until protocol implementation is complete.

use crate::{config::RegionConfig, error::AppError};

pub struct OurNotesClient;

impl OurNotesClient {
    pub async fn application(config: &RegionConfig) -> Result<serde_json::Value, AppError> {
        if !config.enabled {
            return Err(AppError::RegionDisabled);
        }
        Err(AppError::ProtocolPending)
    }
}
