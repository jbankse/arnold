mod bios_process;
mod config;
mod cpu_process;
mod handler;
mod handlers;
mod inbox;
mod jail;
mod jobs;
mod memory;
mod notify;
mod plan_frame;
mod precheck;
mod schema;
mod secrets;
mod session;
mod syscall;
mod uds_server;
mod usage_meter;

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

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--check") {
        let config = ArnoldConfig::load_or_default()?;
        match precheck::run(&config).await {
            Ok(msg) => {
                println!("arnoldd --check OK: {msg}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("arnoldd --check FAILED: {e:#}");
                std::process::exit(1);
            }
        }
    }

    let config = ArnoldConfig::load_or_default()?;
    let arnold_dir = ArnoldConfig::arnold_dir()?;
    let sock_path = ArnoldConfig::socket_path()?;
    let inbox = Inbox::open(&arnold_dir.join("inbox.db"))?;
    let jobs = jobs::JobTable::attach(inbox.connection())?;
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

    let secrets_path = arnold_dir.join("secrets.toml");
    let loaded_secrets = match secrets::Secrets::load(&secrets_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("could not load {}: {e}; starting with empty secrets", secrets_path.display());
            secrets::Secrets::default()
        }
    };
    let env_count = loaded_secrets.as_env_pairs().len();
    if env_count == 0 {
        tracing::warn!(
            "no provider API keys configured; cpu turns will fatal until you set one (use the TUI's Ctrl+S settings, or edit {} directly)",
            secrets_path.display(),
        );
    } else {
        info!("loaded {env_count} provider key(s) from {}", secrets_path.display());
    }

    let state = DaemonState::new(config, jail, memory, jobs, arnold_dir.clone(), loaded_secrets);

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
