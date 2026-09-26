use penlight_notes_api::{api, config::Config};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if !arguments.is_empty() {
        if arguments.len() != 3 || arguments[0] != "master-import" {
            return Err(
                "usage: penlight-notes-api [master-import ENCRYPTED_DIRECTORY OUTPUT_DIRECTORY]"
                    .into(),
            );
        }
        use sirius_api_proxy::master::{import_directory, key_from_hex, MasterDecoder};
        let key = key_from_hex(&std::env::var("SIRIUS_MASTER_KEY_HEX")?)?;
        let iv = key_from_hex(&std::env::var("SIRIUS_MASTER_IV_HEX")?)?;
        let receipt = import_directory(
            std::path::Path::new(&arguments[1]),
            std::path::Path::new(&arguments[2]),
            &MasterDecoder::new(&key, iv),
        )?;
        println!("{}", serde_json::to_string(&receipt)?);
        return Ok(());
    }
    let config = Arc::new(Config::from_env().map_err(std::io::Error::other)?);
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let client = api::load_client(&config).map_err(std::io::Error::other)?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let workers = client
        .as_ref()
        .map(|client| client.start_workers())
        .transpose()
        .map_err(std::io::Error::other)?;
    tracing::info!(address = %listener.local_addr()?, "Penlight Notes API listening");
    axum::serve(listener, api::build_with_client(config, client))
        .with_graceful_shutdown(shutdown())
        .await?;
    if let Some(workers) = workers {
        workers.shutdown().await;
    }
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
