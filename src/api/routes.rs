use axum::{
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Router,
};
use tower_http::trace::TraceLayer;

use super::{handlers, SharedState};
use crate::error::AppError;

pub fn build(state: SharedState) -> Router {
    let api = Router::new()
        .route("/{region}/application", get(handlers::application))
        .fallback(|| async { AppError::NotFound })
        .layer(middleware::from_fn_with_state(state.clone(), authorize));

    Router::new()
        .route("/health", get(handlers::health))
        .route("/version", get(handlers::version))
        .route("/servers", get(handlers::servers))
        .nest("/api", api)
        .fallback(|| async { AppError::NotFound })
        .method_not_allowed_fallback(|| async { AppError::MethodNotAllowed })
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn authorize(
    State(config): State<SharedState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    if let Some(expected) = &config.api_key {
        let key = request
            .headers()
            .get("x-api-key")
            .and_then(|h| h.to_str().ok());
        let bearer = request
            .headers()
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));
        if key != Some(expected.as_str()) && bearer != Some(expected.as_str()) {
            return Err(AppError::Unauthorized);
        }
    }
    Ok(next.run(request).await)
}
