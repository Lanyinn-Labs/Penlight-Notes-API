use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum AppError {
    #[error("unsupported region; expected global or jp")]
    UnsupportedRegion,
    #[error("this region is disabled")]
    RegionDisabled,
    #[error("Our Notes upstream protocol has not been implemented")]
    ProtocolPending,
    #[error("event ID and ranks must be positive; provide 1 to 20 comma-separated ranks")]
    InvalidRankingQuery,
    #[error("Master record ID must be a positive integer")]
    InvalidMasterId,
    #[error("query parameters must contain valid positive IDs or a tab from 0 to 2")]
    InvalidQuery,
    #[error("Sirius upstream request timed out")]
    UpstreamTimeout,
    #[error("game service returned gRPC status {0}")]
    UpstreamGameError(u16),
    #[error("game account session is unavailable")]
    UpstreamAuthenticationUnavailable,
    #[error("game service is rate limiting requests")]
    UpstreamRateLimited,
    #[error("game service is under maintenance")]
    UpstreamMaintenance,
    #[error("game service is unavailable")]
    UpstreamUnavailable,
    #[error("game service returned an invalid response")]
    UpstreamInvalidResponse,
    #[error("decrypted Master snapshot is not configured or is unavailable")]
    MasterDataUnavailable,
    #[error("a valid API key is required")]
    Unauthorized,
    #[error("API request rate limit exceeded")]
    ApiRateLimited,
    #[error("API concurrent request limit exceeded")]
    ApiBusy,
    #[error("API request exceeded its total time limit")]
    ApiTimeout,
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
            Self::InvalidRankingQuery => (StatusCode::BAD_REQUEST, "invalid_ranking_query"),
            Self::InvalidMasterId => (StatusCode::BAD_REQUEST, "invalid_master_id"),
            Self::InvalidQuery => (StatusCode::BAD_REQUEST, "invalid_query"),
            Self::UpstreamTimeout => (StatusCode::GATEWAY_TIMEOUT, "upstream_timeout"),
            Self::UpstreamGameError(_) => (StatusCode::BAD_GATEWAY, "upstream_game_error"),
            Self::UpstreamAuthenticationUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "upstream_authentication_unavailable",
            ),
            Self::UpstreamRateLimited => (StatusCode::SERVICE_UNAVAILABLE, "upstream_rate_limited"),
            Self::UpstreamMaintenance => (StatusCode::SERVICE_UNAVAILABLE, "upstream_maintenance"),
            Self::UpstreamUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "upstream_unavailable"),
            Self::UpstreamInvalidResponse => (StatusCode::BAD_GATEWAY, "upstream_invalid_response"),
            Self::MasterDataUnavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "master_data_unavailable")
            }
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::ApiRateLimited => (StatusCode::TOO_MANY_REQUESTS, "api_rate_limited"),
            Self::ApiBusy => (StatusCode::TOO_MANY_REQUESTS, "api_busy"),
            Self::ApiTimeout => (StatusCode::GATEWAY_TIMEOUT, "api_timeout"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
        };
        let mut response = (
            status,
            Json(json!({"error": {"code": code, "message": self.to_string()}})),
        )
            .into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert("retry-after", axum::http::HeaderValue::from_static("1"));
        }
        response
    }
}
