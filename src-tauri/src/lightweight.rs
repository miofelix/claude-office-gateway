use std::sync::Mutex;
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Normal,
    Entering,
    Lightweight,
    Restoring,
}

#[derive(Default)]
struct Lifecycle {
    phase: Phase,
    reopen: bool,
}
impl Lifecycle {
    fn begin_enter(&mut self) -> bool {
        if self.phase != Phase::Normal {
            return false;
        }
        self.phase = Phase::Entering;
        true
    }
    fn begin_show(&mut self) -> bool {
        match self.phase {
            Phase::Entering => {
                self.reopen = true;
                false
            }
            Phase::Restoring => false,
            _ => {
                self.phase = Phase::Restoring;
                true
            }
        }
    }
    fn destroyed(&mut self) -> bool {
        self.phase = Phase::Lightweight;
        std::mem::take(&mut self.reopen)
    }
    fn restored(&mut self, success: bool) {
        self.phase = if success {
            Phase::Normal
        } else {
            Phase::Lightweight
        };
        self.reopen = false;
    }
}

#[derive(Clone)]
struct Placement {
    position: tauri::PhysicalPosition<i32>,
    size: tauri::PhysicalSize<u32>,
    maximized: bool,
}
#[derive(Default)]
pub struct Windows {
    lifecycle: Mutex<Lifecycle>,
    placement: Mutex<Option<Placement>>,
}
pub fn active(app: &tauri::AppHandle) -> bool {
    app.state::<Windows>().lifecycle.lock().unwrap().phase != Phase::Normal
}

// All native window changes are serialized on the event loop. In particular,
// reopening while destroy() is pending waits for WindowEvent::Destroyed.
pub fn show(app: &tauri::AppHandle) {
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        if let Err(error) = show_now(&handle) {
            log::error!("Could not restore the main window: {error}");
        }
    }) {
        log::error!("Could not schedule the main window: {error}");
    }
}
fn show_now(app: &tauri::AppHandle) -> tauri::Result<()> {
    let state = app.state::<Windows>();
    if !state.lifecycle.lock().unwrap().begin_show() {
        return Ok(());
    }
    let result = (|| {
        #[cfg(target_os = "macos")]
        {
            app.set_activation_policy(tauri::ActivationPolicy::Regular)?;
            app.set_dock_visibility(true)?;
        }
        let window = if let Some(window) = app.get_webview_window("main") {
            window
        } else {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .expect("Main window configuration");
            let window = tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
            if let Some(saved) = state.placement.lock().unwrap().as_ref() {
                let _ = window.set_size(saved.size);
                let _ = window.set_position(saved.position);
                if saved.maximized {
                    let _ = window.maximize();
                }
            }
            window
        };
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        Ok(())
    })();
    state
        .lifecycle
        .lock()
        .unwrap()
        .restored(result.is_ok() || app.get_webview_window("main").is_some());
    crate::tray::refresh(app);
    result
}

pub fn request_enter(app: &tauri::AppHandle) {
    if active(app) {
        show(app);
    } else if let Some(window) = app.get_webview_window("main") {
        // The renderer checks for unsaved drafts and in-flight operations. Keys
        // never need to be persisted to preserve a window that cannot be closed.
        if let Err(error) = window.emit("request-lightweight-mode", ()) {
            log::warn!("Could not request lightweight mode: {error}");
            show(app);
        }
    }
}

#[tauri::command]
pub async fn enter_lightweight_mode(app: tauri::AppHandle) -> Result<(), String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = send.send(enter_now(&handle));
    })
    .map_err(|_| "Could not schedule lightweight mode.")?;
    receive
        .await
        .map_err(|_| "Could not enter lightweight mode.")?
}
fn enter_now(app: &tauri::AppHandle) -> Result<(), String> {
    if app.tray_by_id("office-gateway").is_none() {
        return Err("The tray is unavailable. Keep the main window open.".into());
    }
    if app.state::<crate::updates::Updates>().busy() {
        return Err("Wait for the current operation before entering lightweight mode.".into());
    }
    let state = app.state::<Windows>();
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    if !state.lifecycle.lock().unwrap().begin_enter() {
        return Ok(());
    }
    if let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) {
        *state.placement.lock().unwrap() = Some(Placement {
            position,
            size,
            maximized: window.is_maximized().unwrap_or(false),
        });
    }
    if let Err(error) = window.destroy() {
        state.lifecycle.lock().unwrap().restored(true);
        return Err(format!("Could not release the main window: {error}"));
    }
    crate::tray::refresh(app);
    Ok(())
}
pub fn destroyed(app: &tauri::AppHandle) {
    let reopen = app.state::<Windows>().lifecycle.lock().unwrap().destroyed();
    #[cfg(target_os = "macos")]
    {
        if let Err(error) = app
            .set_dock_visibility(false)
            .and_then(|_| app.set_activation_policy(tauri::ActivationPolicy::Accessory))
        {
            log::warn!("Could not apply tray-only presentation: {error}");
        }
    }
    crate::tray::refresh(app);
    if reopen {
        show(app);
    }
}
#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) {
    show(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reopening_during_window_destruction_waits_and_runs_once() {
        let mut state = Lifecycle::default();
        assert!(state.begin_enter());
        assert!(!state.begin_enter());
        assert!(!state.begin_show());
        assert!(!state.begin_show());
        assert!(state.destroyed());
        assert!(state.begin_show());
        assert!(!state.begin_show());
        state.restored(true);
        assert_eq!(state.phase, Phase::Normal);
        assert!(!state.reopen);
    }
    #[test]
    fn failed_restore_remains_recoverable_and_cold_start_is_normal() {
        let mut state = Lifecycle::default();
        state.begin_enter();
        assert!(!state.destroyed());
        assert!(state.begin_show());
        state.restored(false);
        assert_eq!(state.phase, Phase::Lightweight);
        assert!(state.begin_show());
        state.restored(true);
        assert!(state.begin_enter());
        assert_eq!(Lifecycle::default().phase, Phase::Normal);
    }
}
