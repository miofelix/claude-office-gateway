use crate::AppState;
use semver::Version;
use serde::Serialize;
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const FEED_ROOT: &str = "https://raw.githubusercontent.com/miofelix/claude-office-gateway/updates";
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Installing,
    Restarting,
    Error,
}
#[derive(Debug, Clone, Serialize)]
pub struct Release {
    pub version: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub current_version: String,
    pub channel: &'static str,
    pub phase: Phase,
    pub available: Option<Release>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub checked_at: Option<String>,
    pub error: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            current_version: env!("CARGO_PKG_VERSION").into(),
            channel: channel(env!("CARGO_PKG_VERSION")),
            phase: Phase::Idle,
            available: None,
            downloaded: 0,
            total: None,
            checked_at: None,
            error: None,
        }
    }
}
#[derive(Default)]
pub struct Updates {
    snapshot: Mutex<Snapshot>,
    operation: tokio::sync::Mutex<()>,
}
impl Updates {
    fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }
    fn change(&self, update: impl FnOnce(&mut Snapshot)) {
        update(&mut self.snapshot.lock().unwrap());
    }
    fn fail(&self, message: &str) -> String {
        self.change(|s| {
            s.phase = Phase::Error;
            s.error = Some(message.into());
        });
        message.into()
    }
}
fn channel(version: &str) -> &'static str {
    if Version::parse(version).is_ok_and(|v| !v.pre.is_empty()) {
        "beta"
    } else {
        "stable"
    }
}
fn endpoint(version: &str) -> url::Url {
    format!("{FEED_ROOT}/{}.json", channel(version))
        .parse()
        .expect("Static HTTPS update endpoint")
}
async fn remote(app: &AppHandle) -> Result<Option<Update>, String> {
    let cleanup = app.clone();
    app.updater_builder()
        .endpoints(vec![endpoint(&app.package_info().version.to_string())])
        .map_err(|_| "Update endpoint configuration is invalid.")?
        .timeout(Duration::from_secs(30))
        .on_before_exit(move || {
            cleanup.state::<AppState>().runtime.shutdown();
            if let Some(tray) = cleanup.tray_by_id("office-gateway") {
                let _ = tray.set_visible(false);
            }
            #[cfg(target_os = "windows")]
            tauri_plugin_single_instance::destroy(&cleanup);
        })
        .build()
        .map_err(|_| "Could not initialize signed updates.")?
        .check()
        .await
        .map_err(|_| "Could not check for updates. Check your network and try again.")
        .map_err(String::from)
}
fn release(update: &Update) -> Release {
    Release {
        version: update.version.clone(),
        notes: update.body.clone(),
        published_at: update.date.map(|d| d.to_string()),
    }
}
#[tauri::command]
pub fn get_update_state(state: tauri::State<'_, Updates>) -> Snapshot {
    state.snapshot()
}
#[tauri::command]
pub async fn check_for_updates(
    app: AppHandle,
    state: tauri::State<'_, Updates>,
) -> Result<Snapshot, String> {
    let Ok(_operation) = state.operation.try_lock() else {
        return Ok(state.snapshot());
    };
    state.change(|s| {
        s.phase = Phase::Checking;
        s.error = None;
    });
    let update = remote(&app).await.map_err(|e| state.fail(&e))?;
    state.change(|s| {
        s.checked_at = Some(chrono::Utc::now().to_rfc3339());
        s.available = update.as_ref().map(release);
        s.phase = if update.is_some() {
            Phase::Available
        } else {
            Phase::UpToDate
        };
        s.error = None;
    });
    Ok(state.snapshot())
}
#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: tauri::State<'_, Updates>,
    expected_version: String,
) -> Result<Snapshot, String> {
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "An update operation is already running.")?;
    state.change(|s| {
        s.phase = Phase::Checking;
        s.error = None;
    });
    let update = remote(&app).await.map_err(|e| state.fail(&e))?;
    let Some(mut update) = update else {
        state.change(|s| {
            s.phase = Phase::UpToDate;
            s.available = None;
        });
        return Ok(state.snapshot());
    };
    if update.version != expected_version {
        return Err(
            state.fail("Update details changed. Check for updates again before installing.")
        );
    }
    update.timeout = Some(Duration::from_secs(600));
    state.change(|s| {
        s.phase = Phase::Downloading;
        s.available = Some(release(&update));
        s.downloaded = 0;
        s.total = None;
    });
    // The updater verifies the mandatory Minisign signature before returning bytes.
    let bytes = update
        .download(
            |chunk, total| {
                state.change(|s| {
                    s.downloaded = s.downloaded.saturating_add(chunk as u64);
                    s.total = total;
                })
            },
            || {},
        )
        .await
        .map_err(|_| {
            state.fail("Update download or signature verification failed. Nothing was installed.")
        })?;
    state.change(|s| s.phase = Phase::Installing);
    #[cfg(target_os = "windows")]
    {
        // Windows install launches the installer and exits without RunEvent::Exit.
        app.state::<AppState>().runtime.stop().await;
        update.install(bytes).map_err(|_|state.fail("Could not install the update. The gateway has stopped; reconnect or install from GitHub."))?;
    }
    #[cfg(target_os = "macos")]
    {
        // Keep the existing gateway alive if replacing the app bundle fails.
        update.install(bytes).map_err(|_|state.fail("Could not install the update. Move the app to Applications or install the new version from GitHub."))?;
        app.state::<AppState>().runtime.stop().await;
        state.change(|s| s.phase = Phase::Restarting);
        // The event loop performs plugin cleanup and releases the single-instance lock.
        app.request_restart();
    }
    Ok(state.snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn beta_and_stable_use_distinct_https_channels() {
        assert_eq!(channel("0.2.0-beta.1"), "beta");
        assert_eq!(channel("0.2.0"), "stable");
        assert_eq!(
            endpoint("0.2.0-beta.1").as_str(),
            format!("{FEED_ROOT}/beta.json")
        );
        assert_eq!(endpoint("0.2.0").scheme(), "https");
    }
    #[test]
    fn current_version_is_the_compiled_native_package_version() {
        let state = Updates::default();
        assert_eq!(state.snapshot().current_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(state.snapshot().phase, Phase::Idle);
    }
}
