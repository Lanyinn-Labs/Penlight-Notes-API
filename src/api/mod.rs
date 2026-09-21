mod handlers;
mod routes;

pub use routes::build;
pub type SharedState = std::sync::Arc<crate::config::Config>;
