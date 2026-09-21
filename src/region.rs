use serde::Serialize;

use crate::error::AppError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    Global,
    Jp,
}

impl Region {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "global" => Ok(Self::Global),
            "jp" => Ok(Self::Jp),
            _ => Err(AppError::UnsupportedRegion),
        }
    }
}
