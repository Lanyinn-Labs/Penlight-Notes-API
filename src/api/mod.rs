mod handlers;
mod limits;
mod routes;

pub use routes::build;
pub use routes::build_with_ranking_source;
pub use routes::{build_with_client, load_client};

pub struct ApiState {
    gate: limits::RequestGate,
    pub config: std::sync::Arc<crate::config::Config>,
    pub rankings: crate::ranking::RankingService,
    pub sirius: Option<std::sync::Arc<crate::client::sirius::SiriusClient>>,
}

pub type SharedState = std::sync::Arc<ApiState>;
