use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use crate::syscall::Syscall;
use crate::jail::Jail;
use crate::jobs::JobTable;
use crate::memory::MemoryStore;
use crate::session::SessionState;

pub struct HandlerContext {
    pub jail: Arc<Jail>,
    pub memory: Arc<MemoryStore>,
    pub jobs: JobTable,
    pub bios_binary: PathBuf,
    pub runtime_image: String,
    pub arnold_dir: PathBuf,
    pub session: SessionState,
}

pub async fn dispatch(ctx: &HandlerContext, syscall: Syscall) -> Result<Value> {
    use Syscall::*;
    match syscall {
        Reply { text } => crate::handlers::lifecycle::reply(ctx, text).await,
        Done {} => crate::handlers::lifecycle::done(ctx).await,
        ReadFile { path } => crate::handlers::file::read_file(ctx, path).await,
        WriteFile { path, contents } => crate::handlers::file::write_file(ctx, path, contents).await,
        ReplaceInFile { path, old_string, new_string } =>
            crate::handlers::file::replace_in_file(ctx, path, old_string, new_string).await,
        ListDir { path } => crate::handlers::file::list_dir(ctx, path).await,
        Search { query, path, glob } => crate::handlers::file::search(ctx, query, path, glob).await,
        RunCommand { cmd, args, cwd, timeout_ms, stdin } =>
            crate::handlers::shell::run_command(ctx, cmd, args, cwd, timeout_ms, stdin).await,
        WebFetch { url } => crate::handlers::web::web_fetch(ctx, url).await,
        MemoryRead { topic } => crate::handlers::memory_syscalls::memory_read(ctx, topic).await,
        MemoryWrite { topic, content } => crate::handlers::memory_syscalls::memory_write(ctx, topic, content).await,
        MemoryList {} => crate::handlers::memory_syscalls::memory_list(ctx).await,
        // v0b — implementations in Task 8 / Task 9
        SpinUp439 { prompt, pack_kind, export_workspace_to, env } =>
            crate::handlers::dispatch_439::spin_up_439(ctx, prompt, pack_kind, export_workspace_to, env).await,
        PollJob { job_id } =>
            crate::handlers::dispatch_439::poll_job(ctx, job_id).await,
        SendNotification { title, body, urgency } =>
            crate::handlers::notifications::send_notification(ctx, title, body, urgency).await,
    }
}
