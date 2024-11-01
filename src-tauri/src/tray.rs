use crate::config::get_config;
use crate::windows::set_window_always_on_top;
use crate::ALWAYS_ON_TOP;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::tray::MouseButton;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconEvent,
    Manager, Runtime,
};
use tauri_specta::Event;

#[derive(Serialize, Deserialize, Debug, Clone, specta::Type, tauri_specta::Event)]
pub struct PinnedFromTrayEvent {
    pinned: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, specta::Type, tauri_specta::Event)]
pub struct PinnedFromWindowEvent {
    pinned: bool,
}

impl PinnedFromWindowEvent {
    pub fn pinned(&self) -> &bool {
        &self.pinned
    }
}

pub static TRAY_EVENT_REGISTERED: AtomicBool = AtomicBool::new(false);

pub fn create_tray<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let config = get_config().unwrap();

    let settings_i = MenuItem::with_id(app, "settings", "Settings", true, Some("CmdOrCtrl+,"))?;
    let show_i = MenuItem::with_id(app, "show", "Show", true, config.display_window_hotkey)?;
    let hide_i = PredefinedMenuItem::hide(app, Some("Hide"))?;
    let quit_i = PredefinedMenuItem::quit(app, Some("Quit"))?;
    let pin_i = MenuItem::with_id(app, "pin", "Pin", true, None::<String>)?;

    if ALWAYS_ON_TOP.load(Ordering::Acquire) {
        pin_i.set_text("Unpin").unwrap();
    }

    let tray = app.tray_by_id("tray").unwrap();

    let menu = Menu::with_items(
        app,
        &[
            //
            &settings_i,
            &show_i,
            &hide_i,
            &pin_i,
            &quit_i,
        ],
    )?;

    tray.set_menu(Some(menu.clone()))?;
    let _ = tray.set_show_menu_on_left_click(true);

    if TRAY_EVENT_REGISTERED.load(Ordering::Acquire) {
        return Ok(());
    }

    TRAY_EVENT_REGISTERED.store(true, Ordering::Release);

    tray.on_menu_event(move |app, event| match event.id.as_ref() {
        "settings" => {
            crate::windows::show_window(false, false, true);
        }
        "show" => {
            crate::windows::show_window(false, false, true);
        }
        "hide" => {
            if let Some(window) = app.get_webview_window("main") {
                window.set_focus().unwrap();
                window.unminimize().unwrap();
                window.hide().unwrap();
            }
        }
        "pin" => {
            let pinned = set_window_always_on_top();
            let handle = app.app_handle();
            let pinned_from_tray_event = PinnedFromTrayEvent { pinned };
            pinned_from_tray_event.emit(handle).unwrap_or_default();
            create_tray(app).unwrap();
        }
        "quit" => app.exit(0),
        _ => {}
    });

    tray.on_tray_icon_event(|tray, event| match event {
        TrayIconEvent::Click {
            id,
            position,
            rect,
            button,
            button_state,
        } => {
            if button == MouseButton::Right {
                // crate::windows::show_window(false, false, true);
                let img = tauri::image::Image::from_path("icons/icon.ico").unwrap();

                tray.set_icon(Some(img));
            } else if button == MouseButton::Left {
                // crate::windows::show_window(false, false, true);
            }
        }
        _ => {}
    });

    Ok(())
}
