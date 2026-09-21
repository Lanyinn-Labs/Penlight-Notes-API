use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unsupported region; expected global or jp")]
    UnsupportedRegion,
    #[error("this region is disabled")]
    RegionDisabled,
    #[error("Our Notes upstream protocol has not been implemented")]
    ProtocolPending,
    #[error("a valid API key is required")]
    Unauthorized,
    #[error("route not found")]
    NotFound,
    #[error("method not allowed")]
    MethodNotAllowed,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::UnsupportedRegion => (StatusCode::BAD_REQUEST, "unsupported_region"),
            Self::RegionDisabled => (StatusCode::SERVICE_UNAVAILABLE, "region_disabled"),
            Self::ProtocolPending => (StatusCode::NOT_IMPLEMENTED, "protocol_pending"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
        };
        (
            status,
            Json(json!({"error": {"code": code, "message": self.to_string()}})),
        )
            .into_response()
    }
}
