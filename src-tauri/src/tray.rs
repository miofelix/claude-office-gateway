use crate::{runtime::Phase, AppState};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_opener::OpenerExt;

const REPOSITORY: &str = "https://github.com/miofelix/claude-office-gateway";
struct Controls {
    status: MenuItem<tauri::Wry>,
    open: MenuItem<tauri::Wry>,
    lightweight: CheckMenuItem<tauri::Wry>,
    connect: MenuItem<tauri::Wry>,
    recheck: MenuItem<tauri::Wry>,
    stop: MenuItem<tauri::Wry>,
    copy_url: MenuItem<tauri::Wry>,
    copy_token: MenuItem<tauri::Wry>,
    copy_model: MenuItem<tauri::Wry>,
    github: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Not connected", false, None::<&str>)?;
    let open = MenuItem::with_id(
        app,
        "show",
        "Open Claude Office Gateway",
        true,
        None::<&str>,
    )?;
    let lightweight = CheckMenuItem::with_id(
        app,
        "lightweight",
        "Lightweight mode",
        true,
        false,
        None::<&str>,
    )?;
    let connect = MenuItem::with_id(app, "connect", "Connect gateway", false, None::<&str>)?;
    let recheck = MenuItem::with_id(app, "recheck", "Recheck provider", false, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop gateway", false, None::<&str>)?;
    let copy_url = MenuItem::with_id(app, "copy_url", "Copy Gateway URL", false, None::<&str>)?;
    let copy_token =
        MenuItem::with_id(app, "copy_token", "Copy Gateway token", false, None::<&str>)?;
    let copy_model = MenuItem::with_id(
        app,
        "copy_model",
        "Copy Office model ID",
        false,
        None::<&str>,
    )?;
    let github = MenuItem::with_id(app, "github", "GitHub repository", true, None::<&str>)?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        "Quit Claude Office Gateway",
        true,
        None::<&str>,
    )?;
    let a = PredefinedMenuItem::separator(app)?;
    let b = PredefinedMenuItem::separator(app)?;
    let c = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &open,
            &lightweight,
            &a,
            &connect,
            &recheck,
            &stop,
            &b,
            &copy_url,
            &copy_token,
            &copy_model,
            &c,
            &github,
            &quit,
        ],
    )?;
    #[cfg(target_os = "macos")]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    #[cfg(target_os = "windows")]
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("Application icon missing");
    TrayIconBuilder::with_id("office-gateway")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Claude Office Gateway")
        .menu(&menu)
        .on_menu_event(|app, event| action(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                crate::show(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(Controls {
        status,
        open,
        lightweight,
        connect,
        recheck,
        stop,
        copy_url,
        copy_token,
        copy_model,
        github,
        quit,
    });
    refresh(app.handle());
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let mut previous = String::new();
        loop {
            let state = handle.state::<AppState>();
            let runtime = state.runtime.snapshot();
            let has_key = state
                .vault
                .lock()
                .map(|vault| vault.key.is_some())
                .unwrap_or(false);
            let language = state
                .tray_language
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default();
            let signature = format!(
                "{:?}:{}:{has_key}:{}:{language}",
                runtime.phase,
                runtime.running,
                state
                    .certificate_trusted
                    .load(std::sync::atomic::Ordering::Relaxed)
            );
            if signature != previous {
                refresh(&handle);
                previous = signature;
            }
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        }
    });
    Ok(())
}

pub fn refresh(app: &tauri::AppHandle) {
    let Some(items) = app.try_state::<Controls>() else {
        return;
    };
    let state = app.state::<AppState>();
    let runtime = state.runtime.snapshot();
    let zh = state
        .tray_language
        .lock()
        .map(|s| s.as_str() == "zh")
        .unwrap_or(false);
    let text = |en: &str, cn: &str| if zh { cn.to_string() } else { en.to_string() };
    let phase = match runtime.phase {
        Phase::Stopped => text("Not connected", "未连接"),
        Phase::Starting => text("Starting local gateway…", "正在启动本地网关…"),
        Phase::Checking => text("Checking provider…", "正在检测供应商…"),
        Phase::NeedsTrust => text("Certificate needs attention", "请检查本地证书"),
        Phase::Ready => text("Ready for Office", "Office 连接已就绪"),
        Phase::Failed => text("Needs attention", "需要处理"),
    };
    let busy = matches!(runtime.phase, Phase::Starting | Phase::Checking);
    let can_connect = !runtime.running
        && !busy
        && state.vault.lock().map(|v| v.key.is_some()).unwrap_or(false)
        && state
            .certificate_trusted
            .load(std::sync::atomic::Ordering::Relaxed);
    let _ = items.status.set_text(&phase);
    let _ = items.open.set_text(text(
        "Open Claude Office Gateway",
        "打开 Claude Office Gateway",
    ));
    let _ = items.connect.set_text(text("Connect gateway", "连接网关"));
    let _ = items
        .lightweight
        .set_text(text("Lightweight mode", "轻量模式"));
    let _ = items
        .lightweight
        .set_checked(crate::lightweight::active(app));
    let _ = items.connect.set_enabled(can_connect);
    let _ = items
        .recheck
        .set_text(text("Recheck provider", "重新检测供应商"));
    let _ = items.recheck.set_enabled(runtime.running && !busy);
    let _ = items.stop.set_text(text("Stop gateway", "停止网关"));
    let _ = items.stop.set_enabled(runtime.running || busy);
    let _ = items
        .copy_url
        .set_text(text("Copy Gateway URL", "复制网关地址"));
    let _ = items
        .copy_token
        .set_text(text("Copy Gateway token", "复制网关令牌"));
    let _ = items
        .copy_model
        .set_text(text("Copy Office model ID", "复制 Office 模型 ID"));
    for item in [&items.copy_url, &items.copy_token, &items.copy_model] {
        let _ = item.set_enabled(runtime.phase == Phase::Ready);
    }
    let _ = items
        .github
        .set_text(text("GitHub repository", "GitHub 项目仓库"));
    let _ = items.quit.set_text(text(
        "Quit Claude Office Gateway",
        "退出 Claude Office Gateway",
    ));
    if let Some(tray) = app.tray_by_id("office-gateway") {
        let _ = tray.set_tooltip(Some(format!("Claude Office Gateway · {phase}")));
    }
}

fn action(app: &tauri::AppHandle, id: &str) {
    match id {
        "show" => crate::show(app),
        "lightweight" => {
            refresh(app);
            crate::lightweight::request_enter(app);
        }
        "quit" => app.exit(0),
        "github" => {
            if let Err(error) = app.opener().open_url(REPOSITORY, None::<&str>) {
                log::error!("Could not open project repository: {error}");
            }
        }
        "copy_url" | "copy_token" | "copy_model" => {
            let field = id.strip_prefix("copy_").unwrap();
            if let Err(error) = crate::copy_connection_detail(app.state(), field) {
                log::warn!("Could not copy connection detail: {error}");
                crate::show(app);
            }
        }
        "connect" | "recheck" | "stop" => {
            let handle = app.clone();
            let id = id.to_string();
            tauri::async_runtime::spawn(async move {
                let result = match id.as_str() {
                    "connect" => crate::connect_gateway(handle.state()).await,
                    "recheck" => crate::recheck_gateway(handle.state()).await,
                    _ => crate::stop_gateway(handle.state()).await,
                };
                if let Err(error) = result {
                    log::warn!("Gateway menu action failed: {error}");
                    crate::show(&handle);
                }
                refresh(&handle);
            });
        }
        _ => (),
    }
}
