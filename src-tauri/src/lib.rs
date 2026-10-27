#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("Claude Office Gateway supports macOS and Windows desktop clients only.");
mod certificate;
mod config;
mod credentials;
mod gateway;
mod paths;
mod protocol;
mod runtime;
mod stream;
#[cfg(test)]
mod tests;
mod tray;

use certificate::CertificateManager;
use config::GatewayConfig;
use gateway::GatewayRouter;
use runtime::Runtime;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[derive(Clone, Default)]
struct Vault {
    loaded: bool,
    key: Option<String>,
    token: String,
}
#[derive(Clone, Default)]
pub struct AppState {
    runtime: Runtime,
    vault: Arc<Mutex<Vault>>,
    settings: Arc<Mutex<()>>,
    tray_language: Arc<Mutex<String>>,
    certificate_trusted: Arc<std::sync::atomic::AtomicBool>,
}
#[derive(Serialize)]
struct CertificateState {
    exists: bool,
    trusted: bool,
    subject: &'static str,
}
#[derive(Serialize)]
struct ClientState {
    config: GatewayConfig,
    has_upstream_key: bool,
    gateway_token: String,
    certificate: CertificateState,
    runtime: runtime::Snapshot,
    platform: &'static str,
}
#[derive(Deserialize)]
struct SaveSettingsInput {
    config: GatewayConfig,
    upstream_key: String,
}
fn isolated() -> bool {
    std::env::var("OFFICE_GATEWAY_TEST_MODE").as_deref() == Ok("1")
        && std::env::var_os("OFFICE_GATEWAY_DATA_DIR").is_some()
}
fn load_vault(app: &AppState) -> Result<Vault, String> {
    let mut vault = app
        .vault
        .lock()
        .map_err(|_| "Credential state unavailable")?;
    if !vault.loaded {
        if isolated() {
            vault.token = format!(
                "og_{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
        } else {
            vault.key = credentials::read_upstream_key().map_err(|e| e.to_string())?;
            vault.token = credentials::gateway_token().map_err(|e| e.to_string())?;
        }
        vault.loaded = true;
    }
    Ok(vault.clone())
}
fn snapshot_sync(app: &AppState) -> Result<ClientState, String> {
    let config = GatewayConfig::load(&paths::config_path())
        .map_err(|e| format!("Could not load saved settings: {e}"))?;
    let vault = load_vault(app)?;
    let trusted = CertificateManager::trusted();
    app.certificate_trusted
        .store(trusted, std::sync::atomic::Ordering::Relaxed);
    Ok(ClientState {
        config,
        has_upstream_key: vault.key.is_some(),
        gateway_token: vault.token,
        certificate: CertificateState {
            exists: CertificateManager::exists(),
            trusted,
            subject: certificate::SUBJECT,
        },
        runtime: app.runtime.snapshot(),
        platform: if cfg!(target_os = "macos") {
            "macos"
        } else {
            "windows"
        },
    })
}
async fn snapshot(app: AppState) -> Result<ClientState, String> {
    tauri::async_runtime::spawn_blocking(move || snapshot_sync(&app))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn get_app_state(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    snapshot(app.inner().clone()).await
}
#[tauri::command]
async fn install_certificate(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    if app.runtime.busy() {
        return Err("Stop the gateway before changing its certificate.".into());
    }
    tauri::async_runtime::spawn_blocking(|| {
        CertificateManager::trust_current_user().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(app.inner().clone()).await
}
#[tauri::command]
async fn delete_certificate(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    app.runtime.stop().await;
    tauri::async_runtime::spawn_blocking(|| {
        CertificateManager::delete().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(app.inner().clone()).await
}
#[tauri::command]
async fn save_settings(
    app: tauri::State<'_, AppState>,
    mut input: SaveSettingsInput,
) -> Result<ClientState, String> {
    let owned = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = owned.settings.lock().map_err(|_| "Settings unavailable")?;
        if owned.runtime.busy() {
            return Err("Stop the gateway before editing provider settings.".to_string());
        }
        input.config.base_url = input
            .config
            .base_url
            .trim()
            .trim_end_matches('/')
            .to_string();
        input.config.upstream_model = input.config.upstream_model.trim().to_string();
        input.config.display_name = input.config.display_name.trim().to_string();
        if input.config.display_name == "Custom model" {
            input.config.display_name = input.config.upstream_model.clone();
        }
        input.config.validate().map_err(|e| e.to_string())?;
        let vault = load_vault(&owned)?;
        let key = input.upstream_key.trim();
        if key.is_empty() && vault.key.is_none() {
            return Err("Enter a provider API key.".to_string());
        }
        if !key.is_empty() {
            credentials::validate(key).map_err(|e| e.to_string())?;
        }
        // Validate the complete draft before replacing either saved value.
        if !key.is_empty() && !isolated() {
            credentials::write_upstream_key(key).map_err(|e| e.to_string())?;
        }
        input
            .config
            .save(&paths::config_path())
            .map_err(|e| e.to_string())?;
        if !key.is_empty() {
            owned
                .vault
                .lock()
                .map_err(|_| "Credential state unavailable")?
                .key = Some(key.into());
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(app.inner().clone()).await
}
#[tauri::command]
async fn connect_gateway(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    let trusted = tauri::async_runtime::spawn_blocking(CertificateManager::trusted)
        .await
        .map_err(|e| e.to_string())?;
    if !trusted {
        app.runtime.needs_trust();
        return snapshot(app.inner().clone()).await;
    }
    tauri::async_runtime::spawn_blocking(|| {
        CertificateManager::ensure().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let config = GatewayConfig::load(&paths::config_path()).map_err(|e| e.to_string())?;
    let vault = load_vault(&app)?;
    let key = vault.key.ok_or("Enter a provider API key.")?;
    let router = GatewayRouter::new(config.clone(), vault.token, key).map_err(|e| e.to_string())?;
    app.runtime
        .connect(
            config,
            router,
            &paths::certificate_path(),
            &paths::key_path(),
        )
        .await?;
    snapshot(app.inner().clone()).await
}
#[tauri::command]
async fn stop_gateway(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    app.runtime.stop().await;
    snapshot(app.inner().clone()).await
}
#[tauri::command]
async fn recheck_gateway(app: tauri::State<'_, AppState>) -> Result<ClientState, String> {
    let trusted = tauri::async_runtime::spawn_blocking(CertificateManager::trusted)
        .await
        .map_err(|e| e.to_string())?;
    if !trusted {
        app.runtime.stop().await;
        app.runtime.needs_trust();
    } else {
        app.runtime.recheck().await?;
    }
    snapshot(app.inner().clone()).await
}
#[tauri::command]
fn copy_connection_detail(app: tauri::State<'_, AppState>, field: &str) -> Result<(), String> {
    if app.runtime.snapshot().phase != runtime::Phase::Ready {
        return Err("Connect the gateway before copying Office settings.".into());
    }
    let config = GatewayConfig::load(&paths::config_path()).map_err(|e| e.to_string())?;
    let text = match field {
        "url" => format!("https://localhost:{}", config.port),
        "token" => load_vault(&app)?.token,
        "model" => config.public_model,
        _ => return Err("Unknown connection field".into()),
    };
    arboard::Clipboard::new()
        .map_err(|e| e.to_string())?
        .set_text(text)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn set_tray_language(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    language: String,
) -> Result<(), String> {
    if !matches!(language.as_str(), "en" | "zh") {
        return Err("Unsupported language".into());
    }
    *state
        .tray_language
        .lock()
        .map_err(|_| "Tray language unavailable")? = language;
    tray::refresh(&app);
    Ok(())
}
fn show(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn run() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .manage(AppState::default())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            install_certificate,
            delete_certificate,
            save_settings,
            connect_gateway,
            stop_gateway,
            recheck_gateway,
            copy_connection_detail,
            set_tray_language
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            #[cfg(target_os = "windows")]
            {
                let id: Vec<u16> = app
                    .config()
                    .identifier
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                unsafe {
                    windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
                        id.as_ptr(),
                    );
                }
            }
            tray::setup(app)?;
            let runtime = app.state::<AppState>().runtime.clone();
            let certificate_trusted = app.state::<AppState>().certificate_trusted.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    if runtime.snapshot().running {
                        let trusted =
                            tauri::async_runtime::spawn_blocking(CertificateManager::trusted)
                                .await
                                .unwrap_or(false);
                        certificate_trusted.store(trusted, std::sync::atomic::Ordering::Relaxed);
                        if !trusted {
                            runtime.stop().await;
                            runtime.needs_trust();
                        }
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Could not build Claude Office Gateway")
        .run(|app, event| match event {
            tauri::RunEvent::Exit => {
                app.state::<AppState>().runtime.shutdown();
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => show(app),
            _ => (),
        });
}
