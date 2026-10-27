mod certificate;
mod config;
mod credentials;
mod gateway;
mod paths;

use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use config::GatewayConfig;
use gateway::GatewayRouter;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Mutex, time::Instant};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum GatewayPhase {
    Stopped,
    Starting,
    Checking,
    NeedsTrust,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeState {
    phase: GatewayPhase,
    running: bool,
    verified_at: Option<String>,
    verification_ms: Option<u128>,
    error: Option<String>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            phase: GatewayPhase::Stopped,
            running: false,
            verified_at: None,
            verification_ms: None,
            error: None,
        }
    }
}

struct RuntimeControl {
    state: RuntimeState,
    handle: Option<Handle>,
}
pub struct AppState {
    runtime: Mutex<RuntimeControl>,
}

#[derive(Debug, Clone, Serialize)]
struct CertificateState {
    exists: bool,
    trusted: bool,
    subject: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ClientState {
    config: GatewayConfig,
    has_upstream_key: bool,
    gateway_token: String,
    certificate: CertificateState,
    runtime: RuntimeState,
    platform: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
struct SaveSettingsInput {
    config: GatewayConfig,
    upstream_key: String,
}

fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "windows"
    }
}

fn load_config() -> anyhow::Result<GatewayConfig> {
    GatewayConfig::load(&paths::config_path())
}

fn state_snapshot(app: &AppState) -> Result<ClientState, String> {
    let config = load_config().map_err(|error| error.to_string())?;
    let has_upstream_key = credentials::read_upstream_key()
        .map_err(|error| error.to_string())?
        .is_some();
    let gateway_token = credentials::gateway_token().map_err(|error| error.to_string())?;
    let runtime = app
        .runtime
        .lock()
        .map_err(|_| "Runtime state is unavailable".to_string())?
        .state
        .clone();
    Ok(ClientState {
        config,
        has_upstream_key,
        gateway_token,
        certificate: CertificateState {
            exists: certificate::CertificateManager::exists(),
            trusted: certificate::CertificateManager::trusted(),
            subject: "Claude Office Gateway localhost",
        },
        runtime,
        platform: current_platform(),
    })
}

fn set_runtime(app: &AppState, update: impl FnOnce(&mut RuntimeControl)) -> Result<(), String> {
    let mut runtime = app
        .runtime
        .lock()
        .map_err(|_| "Runtime state is unavailable".to_string())?;
    update(&mut runtime);
    Ok(())
}

#[tauri::command]
fn get_app_state(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    state_snapshot(&app)
}

#[tauri::command]
fn install_certificate(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    certificate::CertificateManager::trust_current_user().map_err(|error| error.to_string())?;
    state_snapshot(&app)
}

#[tauri::command]
fn delete_certificate(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    stop_runtime(&app)?;
    certificate::CertificateManager::delete().map_err(|error| error.to_string())?;
    state_snapshot(&app)
}

#[tauri::command]
fn save_settings(
    app: tauri::State<'_, AppState>,
    input: SaveSettingsInput,
) -> Result<ClientState, String> {
    if input.upstream_key.trim().is_empty() {
        if !input.config.base_url.trim().is_empty()
            && credentials::read_upstream_key()
                .map_err(|error| error.to_string())?
                .is_none()
        {
            return Err("Enter a provider API key.".into());
        }
    } else {
        credentials::write_upstream_key(input.upstream_key.trim())
            .map_err(|error| error.to_string())?;
    }
    input
        .config
        .save(&paths::config_path())
        .map_err(|error| error.to_string())?;
    credentials::gateway_token().map_err(|error| error.to_string())?;
    state_snapshot(&app)
}

#[tauri::command]
async fn connect_gateway(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    if !certificate::CertificateManager::trusted() {
        set_runtime(&app, |runtime| {
            runtime.state.phase = GatewayPhase::NeedsTrust;
            runtime.state.error =
                Some("Install and trust the local certificate before connecting.".into());
        })?;
        return state_snapshot(&app);
    }
    stop_runtime(&app)?;
    let config = load_config().map_err(|error| error.to_string())?;
    let key = credentials::read_upstream_key()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Enter a provider API key.".to_string())?;
    let token = credentials::gateway_token().map_err(|error| error.to_string())?;
    let router =
        GatewayRouter::new(config.clone(), token, key).map_err(|error| error.to_string())?;
    set_runtime(&app, |runtime| {
        runtime.state = RuntimeState {
            phase: GatewayPhase::Starting,
            running: false,
            verified_at: None,
            verification_ms: None,
            error: None,
        };
    })?;
    let tls = RustlsConfig::from_pem_file(paths::certificate_path(), paths::key_path())
        .await
        .map_err(|error| error.to_string())?;
    let address = SocketAddr::from(([127, 0, 0, 1], config.port));
    let handle = Handle::new();
    let server_handle = handle.clone();
    let app_router = router.clone().app();
    tokio::spawn(async move {
        let result = axum_server::bind_rustls(address, tls)
            .handle(server_handle)
            .serve(app_router.into_make_service())
            .await;
        if let Err(error) = result {
            log::error!("Gateway listener stopped: {error}");
        }
    });
    set_runtime(&app, |runtime| {
        runtime.handle = Some(handle);
        runtime.state.phase = GatewayPhase::Checking;
        runtime.state.running = true;
    })?;
    let started = Instant::now();
    if let Err(error) = router.verify_upstream().await {
        stop_runtime(&app)?;
        set_runtime(&app, |runtime| {
            runtime.state.phase = GatewayPhase::Failed;
            runtime.state.error = Some(error.to_string());
        })?;
    } else {
        set_runtime(&app, |runtime| {
            runtime.state.phase = GatewayPhase::Ready;
            runtime.state.running = true;
            runtime.state.verification_ms = Some(started.elapsed().as_millis());
            runtime.state.verified_at = Some(chrono::Utc::now().to_rfc3339());
        })?;
    }
    state_snapshot(&app)
}

#[tauri::command]
fn stop_gateway(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    stop_runtime(&app)?;
    state_snapshot(&app)
}

#[tauri::command]
async fn recheck_gateway(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    let config = load_config().map_err(|error| error.to_string())?;
    let key = credentials::read_upstream_key()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Enter a provider API key.".to_string())?;
    let token = credentials::gateway_token().map_err(|error| error.to_string())?;
    let router = GatewayRouter::new(config, token, key).map_err(|error| error.to_string())?;
    set_runtime(&app, |runtime| {
        runtime.state.phase = GatewayPhase::Checking;
        runtime.state.error = None;
    })?;
    let started = Instant::now();
    match router.verify_upstream().await {
        Ok(()) => set_runtime(&app, |runtime| {
            runtime.state.phase = GatewayPhase::Ready;
            runtime.state.verification_ms = Some(started.elapsed().as_millis());
            runtime.state.verified_at = Some(chrono::Utc::now().to_rfc3339());
        })?,
        Err(error) => set_runtime(&app, |runtime| {
            runtime.state.phase = GatewayPhase::Failed;
            runtime.state.error = Some(error.to_string());
        })?,
    }
    state_snapshot(&app)
}

fn stop_runtime(app: &AppState) -> Result<(), String> {
    set_runtime(app, |runtime| {
        if let Some(handle) = runtime.handle.take() {
            handle.shutdown();
        }
        runtime.state = RuntimeState::default();
    })
}

#[cfg(target_os = "windows")]
fn set_windows_app_user_model_id(app: &tauri::AppHandle) {
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    let id: Vec<u16> = app
        .config()
        .identifier
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState {
        runtime: Mutex::new(RuntimeControl {
            state: RuntimeState::default(),
            handle: None,
        }),
    };
    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            install_certificate,
            delete_certificate,
            save_settings,
            connect_gateway,
            stop_gateway,
            recheck_gateway
        ])
        .setup(|app| {
            #[cfg(target_os = "windows")]
            set_windows_app_user_model_id(app.handle());
            let show = MenuItem::with_id(app, "show", "Open settings", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::new()
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = stop_runtime(&state);
                }
            }
        });
}
