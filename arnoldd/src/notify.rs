use anyhow::{anyhow, Result};
use std::process::Stdio;
use tokio::process::Command;

/// Dispatch an OS notification via the platform-native CLI.
///
/// On macOS, shells out to `osascript`. On Linux, shells out to `notify-send`.
/// On other platforms, returns an error.
///
/// The `urgency` parameter is a freeform string with defined values:
/// "low" | "normal" | "critical" (matching freedesktop standards). macOS
/// ignores urgency; Linux passes it through to notify-send.
pub async fn dispatch(title: &str, body: &str, _urgency: Option<&str>) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification {body} with title {title}",
            body = escape_applescript(body),
            title = escape_applescript(title),
        );
        let status = Command::new("osascript")
            .arg("-e").arg(&script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .status()
            .await
            .map_err(|e| anyhow!("osascript not found: {e}"))?;
        if !status.success() {
            return Err(anyhow!("osascript failed with exit {:?}", status.code()));
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("notify-send");
        if let Some(u) = _urgency {
            cmd.arg("--urgency").arg(u);
        }
        cmd.arg(title).arg(body)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let status = cmd.status().await
            .map_err(|e| anyhow!("notify-send not found: {e}"))?;
        if !status.success() {
            return Err(anyhow!("notify-send failed with exit {:?}", status.code()));
        }
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(anyhow!("OS notifications are only supported on macOS and Linux in v0b"))
}

#[cfg(target_os = "macos")]
fn escape_applescript(s: &str) -> String {
    // Double-quote and escape backslashes/quotes for AppleScript string literals.
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
