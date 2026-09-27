use penlight_notes_api::{api, config::Config};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() == 2 && arguments[0] == "check-client-config" {
        let client = penlight_notes_api::client_release::ClientRelease::read(std::path::Path::new(
            &arguments[1],
        ))
        .map_err(std::io::Error::other)?;
        client.validate_protocol().map_err(std::io::Error::other)?;
        println!("Client configuration valid");
        return Ok(());
    }
    if arguments == ["check-config"] {
        let config = Config::from_env().map_err(std::io::Error::other)?;
        api::load_client(&config).map_err(std::io::Error::other)?;
        println!("Configuration valid");
        return Ok(());
    }
    if arguments == ["master-update"] {
        let config = Config::from_env().map_err(std::io::Error::other)?;
        let client = api::load_client(&config)
            .map_err(std::io::Error::other)?
            .ok_or("JP protocol configuration is required")?;
        let settings = config
            .region(penlight_notes_api::region::Region::Jp)
            .sirius
            .as_ref()
            .ok_or("JP protocol configuration is required")?;
        let updater =
            sirius_api_proxy::master_update::MasterUpdater::new(settings, client.core.clone())
                .map_err(|_| "Master update configuration or secrets are unavailable")?;
        let result = updater.update_once().await?;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    if !arguments.is_empty() {
        if arguments.len() != 3 || arguments[0] != "master-import" {
            return Err(
                "usage: penlight-notes-api [check-config | check-client-config PATH | master-update | master-import ENCRYPTED_DIRECTORY OUTPUT_DIRECTORY]"
                    .into(),
            );
        }
        use sirius_api_proxy::master::{import_directory, key_from_hex, MasterDecoder};
        penlight_notes_api::config::load_dotenv().map_err(std::io::Error::other)?;
        penlight_notes_api::client_release::ClientRelease::load()
            .map_err(std::io::Error::other)?
            .install_defaults();
        let key = key_from_hex(&std::env::var("PENLIGHT_MASTER_KEY_HEX")?)?;
        let iv = key_from_hex(&std::env::var("PENLIGHT_MASTER_IV_HEX")?)?;
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
    let worker = client
        .as_ref()
        .map(|client| client.start_worker())
        .transpose()
        .map_err(std::io::Error::other)?
        .flatten();
    tracing::info!(address = %listener.local_addr()?, "Penlight Notes API listening");
    axum::serve(listener, api::build_with_client(config, client))
        .with_graceful_shutdown(shutdown())
        .await?;
    if let Some(worker) = worker {
        worker.shutdown().await;
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
