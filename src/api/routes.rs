use axum::{
    extract::{Path, Request, State},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Router,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use super::{handlers, ApiState, SharedState};
use crate::{
    client::sirius::{SiriusClient, SiriusRankingSource},
    config::Config,
    error::AppError,
    offline_master,
    ranking::{RankingService, RankingSource},
};

pub fn build(config: Arc<Config>) -> Router {
    let client = load_client(&config).expect("protocol client initialization failed");
    build_with_client(config, client)
}

pub fn load_client(config: &Config) -> Result<Option<Arc<SiriusClient>>, String> {
    config
        .region(crate::region::Region::Jp)
        .sirius
        .clone()
        .map(SiriusClient::new)
        .transpose()
        .map(|client| client.map(Arc::new))
}

pub fn build_with_client(config: Arc<Config>, client: Option<Arc<SiriusClient>>) -> Router {
    assemble(
        config,
        Arc::new(SiriusRankingSource(client.clone())),
        client,
    )
}

pub fn build_with_ranking_source(config: Arc<Config>, source: Arc<dyn RankingSource>) -> Router {
    let client = load_client(&config).expect("protocol client initialization failed");
    assemble(config, source, client)
}

fn assemble(
    config: Arc<Config>,
    source: Arc<dyn RankingSource>,
    client: Option<Arc<SiriusClient>>,
) -> Router {
    let state = Arc::new(ApiState {
        rankings: RankingService::new(source, config.ranking_cache),
        sirius: client,
        config,
    });
    let mut api = Router::new()
        .route("/{region}/application", get(handlers::application))
        .route("/{region}/announcements", get(handlers::announcements))
        .route("/{region}/announcements/{id}", get(handlers::announcement))
        .route(
            "/{region}/players/by-profile-id/{id}",
            get(handlers::player_profile),
        )
        .route(
            "/{region}/events/{event_id}/rankings",
            get(handlers::event_rankings),
        )
        .route(
            "/{region}/events/{event_id}/players/{player_id}/deck",
            get(handlers::event_deck),
        )
        .route(
            "/{region}/music/{id}/rankings",
            get(handlers::music_rankings),
        )
        .route(
            "/{region}/challenge-music/{id}/rankings",
            get(handlers::challenge_rankings),
        )
        .route("/{region}/user/account", get(handlers::account))
        .route("/{region}/user/data", get(handlers::user_data))
        .route(
            "/{region}/events/{event_id}/cutoffs",
            get(handlers::event_cutoffs),
        )
        .route("/{region}/master-schema", get(handlers::master_schema_list))
        .route("/{region}/master-data", get(handlers::master_data))
        .route("/{region}/master-updater", get(handlers::master_updater))
        .route("/{region}/master/{table}", get(handlers::master_records))
        .route(
            "/{region}/master-schema/{table}",
            get(handlers::master_schema),
        );
    for &(resource, field) in handlers::USER_RESOURCES {
        api = api.route(
            &format!("/{{region}}/user/{resource}"),
            get(
                move |State(state): State<SharedState>, Path(region): Path<String>| async move {
                    handlers::user_resource(state, region, field).await
                },
            ),
        );
    }
    for &resource in offline_master::JP_RESOURCES {
        api = api
            .route(
                &format!("/{{region}}/{resource}"),
                get(
                    move |State(state): State<SharedState>, Path(region): Path<String>| async move {
                        handlers::jp_catalog_list(state, region, resource).await
                    },
                ),
            )
            .route(
                &format!("/{{region}}/{resource}/{{id}}"),
                get(
                    move |State(state): State<SharedState>,
                          Path((region, id)): Path<(String, String)>| async move {
                        handlers::jp_catalog_entry(state, region, resource, id).await
                    },
                ),
            );
    }
    let api = api
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
    if let Some(expected) = &config.config.api_key {
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
