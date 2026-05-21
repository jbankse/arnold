mod config;
mod cpu_process;
mod handler;
mod handlers;
mod inbox;
mod jail;
mod jobs;
mod memory;
mod plan_frame;
mod schema;
mod session;
mod syscall;
mod usage_meter;
mod uds_server;

use anyhow::Result;
use tracing::{info, error};
use config::ArnoldConfig;
use inbox::Inbox;
use jail::Jail;
use memory::MemoryStore;
use session::DaemonState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("arnoldd=info".parse()?))
        .init();

    let config = ArnoldConfig::load_or_default()?;
    let arnold_dir = ArnoldConfig::arnold_dir()?;
    let sock_path = ArnoldConfig::socket_path()?;
    let inbox = Inbox::open(&arnold_dir.join("inbox.db"))?;
    let memory = MemoryStore::open(&arnold_dir.join("memory"))?;
    let mut jail_roots = vec![arnold_dir.join("workspace")];
    jail_roots.extend(config.allowed_dirs.iter().cloned());
    std::fs::create_dir_all(&jail_roots[0])?;
    for p in &config.allowed_dirs {
        if let Err(e) = p.canonicalize() {
            tracing::warn!(
                "config.allowed_dirs entry {} cannot be canonicalized ({e}); jail roots use the literal path and may never match real candidate paths",
                p.display(),
            );
        }
    }
    let jail = Jail::new(jail_roots);

    let state = DaemonState::new(config, jail, memory);

    info!(socket = %sock_path.display(), "arnoldd starting");
    let listener = uds_server::bind(&sock_path).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let s = state.clone();
        let i = inbox.clone();
        tokio::spawn(async move {
            if let Err(e) = uds_server::handle_connection(stream, s, i).await {
                error!("connection handler error: {e:#}");
            }
        });
    }
}
