use crate::config::get_config;
use crate::recorder;
use crate::windows::set_window_always_on_top;
use crate::ALWAYS_ON_TOP;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use xcap::Monitor;

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
    let _ = tray.set_show_menu_on_left_click(false);

    println!("create tray");
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

    let event_handled = Arc::new(Mutex::new(false)); // Use Arc<Mutex> for thread safety

    tray.on_tray_icon_event(move |tray, event| match event {
        TrayIconEvent::Click {
            id,
            position,
            rect,
            button,
            button_state,
        } => {
            if button == MouseButton::Right {
                // crate::windows::show_window(false, false, true);
            } else if button == MouseButton::Left {
                let mut handled = event_handled.lock().unwrap(); // Lock the mutex to access the flag
                if *handled {
                    return;
                }

                if !*handled {
                    *handled = true; // Set the flag

                    let event_handled_clone = Arc::clone(&event_handled);

                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(300)); // Adjust as needed
                        let mut handled = event_handled_clone.lock().unwrap(); // Lock again to reset the flag
                        *handled = false; // Reset the flag
                    });
                }

                if crate::RECORDING.load(Ordering::Acquire) {
                    println!("stop recording.",);
                    set_recording_icon(tray, false);
                    recorder::stop_record();

                    crate::RECORDING.store(false, Ordering::Release);
                    return;
                }

                let configured = get_config().unwrap().configured.unwrap_or(false);
                if !configured {
                    println!("not configured yet");
                    crate::windows::show_window(false, false, true);
                } else {
                    println!("configured");
                    set_recording_icon(tray, true);
                    if let Some(monitor) = Monitor::all().unwrap().get(0) {
                        println!("start recording.",);
                        let monitor = monitor.clone();
                        std::thread::spawn(|| {
                            recorder::record(monitor);
                        });
                    } else {
                        println!("no monitor");
                    }

                    crate::RECORDING.store(true, Ordering::Release);
                }
            }
        }
        _ => {}
    });

    Ok(())
}

fn set_recording_icon<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, recording: bool) {
    let mut path = "icons/recorder.png";
    if recording {
        path = "icons/recording.png";
    }
    let img = tauri::image::Image::from_path(path).unwrap();
    tray.set_icon(Some(img)).expect("set_icon failed");
}
