mod config;
mod inbox;
mod uds_server;

use anyhow::Result;
use tracing::{info, error};
use config::ArnoldConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("arnoldd=info".parse::<tracing_subscriber::filter::Directive>()?))
        .init();

    let _config = ArnoldConfig::load_or_default()?;
    let sock_path = ArnoldConfig::socket_path()?;
    info!(socket = %sock_path.display(), "arnoldd starting");

    let listener = uds_server::bind(&sock_path).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = uds_server::handle_connection(stream).await {
                error!("connection handler error: {e:#}");
            }
        });
    }
}
