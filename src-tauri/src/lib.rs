use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder, Manager};

#[tauri::command]
fn ping() -> &'static str {
    "ok"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![ping]);

    builder = builder.setup(|app| {
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
    });

    builder.run(tauri::generate_context!()).expect("error while running application");
}

