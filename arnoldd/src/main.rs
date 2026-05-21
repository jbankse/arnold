mod config;
mod inbox;
mod jail;
mod memory;
mod schema;
mod session;
mod syscall;
mod uds_server;

use anyhow::Result;
use tracing::{info, error};
use config::ArnoldConfig;
use inbox::Inbox;
use session::SessionRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("arnoldd=info".parse()?))
        .init();

    let _config = ArnoldConfig::load_or_default()?;
    let sock_path = ArnoldConfig::socket_path()?;
    let inbox = Inbox::open(&ArnoldConfig::arnold_dir()?.join("inbox.db"))?;
    let sessions = SessionRegistry::default();

    info!(socket = %sock_path.display(), "arnoldd starting");
    let listener = uds_server::bind(&sock_path).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let s = sessions.clone();
        let i = inbox.clone();
        tokio::spawn(async move {
            if let Err(e) = uds_server::handle_connection(stream, s, i).await {
                error!("connection handler error: {e:#}");
            }
        });
    }
}
