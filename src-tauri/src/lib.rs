#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("Claude Office Gateway supports macOS and Windows desktop clients only.");
mod certificate;
mod config;
mod credentials;
mod gateway;
mod lightweight;
mod paths;
mod protocol;
mod providers;
mod runtime;
mod stream;
#[cfg(test)]
mod tests;
mod transport;
mod tray;
mod updates;
mod upstream_errors;

use certificate::CertificateManager;
use config::{GatewayConfig, ProviderStore};
use gateway::GatewayRouter;
use runtime::Runtime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[derive(Clone, Default)]
struct Vault {
    provider_id: Option<String>,
    key: Option<String>,
    keys: HashMap<String, Option<String>>,
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
    active_provider_id: String,
    providers: Vec<ProviderSummary>,
    config: GatewayConfig,
    has_upstream_key: bool,
    gateway_token: String,
    certificate: CertificateState,
    runtime: runtime::Snapshot,
    platform: &'static str,
    test_mode: bool,
}
#[derive(Serialize)]
struct ProviderSummary {
    id: String,
    name: String,
    base_url: String,
    upstream_model: String,
    public_model: String,
}
#[derive(Deserialize)]
struct SaveSettingsInput {
    provider_id: Option<String>,
    name: String,
    config: GatewayConfig,
    upstream_key: String,
}
#[derive(Deserialize)]
struct ProviderIdInput {
    provider_id: String,
}
fn isolated() -> bool {
    std::env::var("OFFICE_GATEWAY_TEST_MODE").as_deref() == Ok("1")
        && std::env::var_os("OFFICE_GATEWAY_DATA_DIR").is_some()
}
fn load_vault(app: &AppState, provider_id: &str) -> Result<Vault, String> {
    let mut vault = app
        .vault
        .lock()
        .map_err(|_| "Credential state unavailable")?;
    if vault.token.is_empty() {
        vault.token = if isolated() {
            format!(
                "og_{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            )
        } else {
            credentials::gateway_token().map_err(|e| e.to_string())?
        };
    }
    if vault.provider_id.as_deref() != Some(provider_id) {
        let key = if let Some(key) = vault.keys.get(provider_id) {
            key.clone()
        } else {
            let key = if isolated() {
                None
            } else {
                credentials::read_upstream_key(provider_id).map_err(|e| e.to_string())?
            };
            vault.keys.insert(provider_id.to_string(), key.clone());
            key
        };
        vault.provider_id = Some(provider_id.to_string());
        vault.key = key;
    }
    Ok(vault.clone())
}
fn snapshot_sync(app: &AppState) -> Result<ClientState, String> {
    let store = ProviderStore::load(&paths::config_path())
        .map_err(|e| format!("Could not load provider settings: {e}"))?;
    let active = store.active().map_err(|e| e.to_string())?;
    let vault = load_vault(app, &active.id)?;
    let trusted = CertificateManager::trusted();
    app.certificate_trusted
        .store(trusted, std::sync::atomic::Ordering::Relaxed);
    Ok(ClientState {
        active_provider_id: active.id.clone(),
        providers: store
            .providers
            .iter()
            .map(|provider| ProviderSummary {
                id: provider.id.clone(),
                name: provider.name.clone(),
                base_url: provider.config.base_url.clone(),
                upstream_model: provider.config.upstream_model.clone(),
                public_model: provider.config.public_model.clone(),
            })
            .collect(),
        test_mode: isolated(),
        config: active.config.clone(),
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
        input.name = input.name.trim().to_string();
        if input.name.is_empty() || input.name.chars().count() > 80 {
            return Err("Enter a provider name up to 80 characters.".to_string());
        }
        input.config.validate().map_err(|e| e.to_string())?;
        let mut store = ProviderStore::load(&paths::config_path()).map_err(|e| e.to_string())?;
        let provider_id = match input.provider_id.take() {
            Some(id) if store.providers.iter().any(|provider| provider.id == id) => id,
            Some(_) => return Err("Provider profile was not found.".to_string()),
            None if store.providers.len() == 1
                && store.providers[0].id == "default"
                && store.providers[0].config.base_url.trim().is_empty()
                && store.providers[0].config.upstream_model.trim().is_empty() =>
            {
                "default".into()
            }
            None => format!("provider-{}", uuid::Uuid::new_v4().simple()),
        };
        let vault = load_vault(&owned, &provider_id)?;
        let key = input.upstream_key.trim();
        if key.is_empty() && vault.key.is_none() {
            return Err("Enter a provider API key.".to_string());
        }
        if !key.is_empty() {
            credentials::validate(key).map_err(|e| e.to_string())?;
        }
        // Validate the complete draft before replacing either saved value.
        if !key.is_empty() && !isolated() {
            credentials::write_upstream_key(&provider_id, key).map_err(|e| e.to_string())?;
        }
        if let Some(profile) = store
            .providers
            .iter_mut()
            .find(|profile| profile.id == provider_id)
        {
            profile.name = input.name;
            profile.config = input.config;
        } else {
            store.providers.push(config::ProviderProfile {
                id: provider_id.clone(),
                name: input.name,
                config: input.config,
            });
        }
        store.active_provider_id = provider_id.clone();
        store
            .save(&paths::config_path())
            .map_err(|e| e.to_string())?;
        if !key.is_empty() {
            let mut vault = owned
                .vault
                .lock()
                .map_err(|_| "Credential state unavailable")?;
            vault.keys.insert(provider_id.clone(), Some(key.into()));
            vault.provider_id = Some(provider_id);
            vault.key = Some(key.into());
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(app.inner().clone()).await
}
#[tauri::command]
async fn select_provider(
    app: tauri::State<'_, AppState>,
    input: ProviderIdInput,
) -> Result<ClientState, String> {
    if app.runtime.snapshot().running || app.runtime.busy() {
        return Err("Stop the gateway before switching provider profiles.".into());
    }
    let owned = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = owned.settings.lock().map_err(|_| "Settings unavailable")?;
        let mut store = ProviderStore::load(&paths::config_path()).map_err(|e| e.to_string())?;
        if !store
            .providers
            .iter()
            .any(|provider| provider.id == input.provider_id)
        {
            return Err("Provider profile was not found.".to_string());
        }
        store.active_provider_id = input.provider_id;
        store.save(&paths::config_path()).map_err(|e| e.to_string())
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
    let store = ProviderStore::load(&paths::config_path()).map_err(|e| e.to_string())?;
    let active = store.active().map_err(|e| e.to_string())?;
    let config = active.config.clone();
    let vault = load_vault(&app, &active.id)?;
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
    let store = ProviderStore::load(&paths::config_path()).map_err(|e| e.to_string())?;
    let active = store.active().map_err(|e| e.to_string())?;
    let text = match field {
        "url" => format!("https://localhost:{}", active.config.port),
        "token" => load_vault(&app, &active.id)?.token,
        "model" => active.config.public_model.clone(),
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
    lightweight::show(app);
}

pub fn run() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .manage(AppState::default())
        .manage(updates::Updates::default())
        .manage(lightweight::Windows::default())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            install_certificate,
            delete_certificate,
            save_settings,
            select_provider,
            connect_gateway,
            stop_gateway,
            recheck_gateway,
            copy_connection_detail,
            set_tray_language,
            lightweight::enter_lightweight_mode,
            lightweight::show_main_window,
            updates::get_update_state,
            updates::check_for_updates,
            updates::install_app_update
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                lightweight::destroyed(window.app_handle());
            }
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
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } if lightweight::active(app) => {
                api.prevent_exit();
            }
            tauri::RunEvent::Exit => {
                app.state::<AppState>().runtime.shutdown();
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => show(app),
            _ => (),
        });
}
