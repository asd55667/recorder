// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod tray;
mod windows;

use crate::config::{clear_config_cache, get_config_content};
use crate::windows::get_window_always_on_top;

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri_plugin_notification::NotificationExt;
use tauri_specta::Event;
use tray::{PinnedFromTrayEvent, PinnedFromWindowEvent};

#[cfg(target_os = "macos")]
fn query_accessibility_permissions() -> bool {
    let trusted = macos_accessibility_client::accessibility::application_is_trusted_with_prompt();
    if trusted {
        print!("Application is totally trusted!");
    } else {
        print!("Application isn't trusted :(");
    }
    trusted
}

#[cfg(not(target_os = "macos"))]
fn query_accessibility_permissions() -> bool {
    return true;
}

pub static CPU_VENDOR: Mutex<String> = Mutex::new(String::new());
pub static APP_HANDLE: once_cell::sync::OnceCell<tauri::AppHandle> =
    once_cell::sync::OnceCell::new();
pub static ALWAYS_ON_TOP: AtomicBool = AtomicBool::new(false);

fn main() {
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_all(); // Refreshing CPU information.
    if let Some(cpu) = sys.cpus().first() {
        let vendor_id = cpu.vendor_id().to_string();
        *CPU_VENDOR.lock() = vendor_id;
    }

    let builder = tauri_specta::Builder::<tauri::Wry>::new()
        // Then register them (separated by a comma)
        .commands(tauri_specta::collect_commands![
            get_config_content,
            clear_config_cache,
            get_window_always_on_top,
        ])
        .events(tauri_specta::collect_events![
            PinnedFromWindowEvent,
            PinnedFromTrayEvent,
        ]);

    #[cfg(debug_assertions)] // <- Only export on non-release builds
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    let mut app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            println!("{}, {argv:?}, {cwd}", app.package_info().name);

            app.notification()
                .builder()
                .title("This app is already running!")
                .body("You can find it in the tray menu.")
                .show()
                .unwrap();
        }))
        .plugin(tauri_plugin_process::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            let app_handle = app.handle();
            APP_HANDLE.get_or_init(|| app.handle().clone());
            tray::create_tray(&app_handle)?;
            app_handle.plugin(tauri_plugin_global_shortcut::Builder::new().build())?;

            if !query_accessibility_permissions() {
                app.notification()
                    .builder()
                    .title("Accessibility permissions")
                    .body("Please grant accessibility permissions to the app")
                    .icon("icon.png")
                    .show()
                    .unwrap();
            }

            builder.mount_events(app);

            let handle = app_handle.clone();
            PinnedFromWindowEvent::listen_any(app_handle, move |event| {
                let pinned = event.payload.pinned();
                ALWAYS_ON_TOP.store(*pinned, Ordering::Release);
                tray::create_tray(&handle).unwrap();
            });

            let handle = app_handle.clone();
            ConfigUpdatedEvent::listen_any(app_handle, move |_event| {
                clear_config_cache();
                tray::create_tray(&handle).unwrap();
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    #[cfg(target_os = "macos")]
    {
        app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    }

    app.run(|_app, event| match event {
        tauri::RunEvent::Exit => {
            println!("\nexit.");
        }
        tauri::RunEvent::Ready => {
            println!("\nready.");
            windows::close_main_window();
        }
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            println!("window label: {}", label);
            if label == "main" {
                // api.prevent_close();
            }
        }
        // https://github.com/tauri-apps/tauri/discussions/6308
        // Keep the event loop running even if all windows are closed
        // This allow us to catch system tray events when there is no window
        tauri::RunEvent::ExitRequested { api, .. } => {
            api.prevent_exit();
        }
        _ => {}
    });
}
