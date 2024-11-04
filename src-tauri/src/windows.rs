use crate::config;
use crate::ALWAYS_ON_TOP;
use crate::APP_HANDLE;

use std::sync::atomic::Ordering;

use cocoa::appkit::NSWindow;
use debug_print::debug_println;
use mouse_position::mouse_position::Mouse;
use tauri::{LogicalPosition, Manager, PhysicalPosition};

#[tauri::command]
#[specta::specta]
pub fn get_window_always_on_top() -> bool {
    ALWAYS_ON_TOP.load(Ordering::Acquire)
}

pub fn close_main_window() {
    if let Some(handle) = APP_HANDLE.get() {
        match handle.get_webview_window("main") {
            Some(window) => {
                #[cfg(not(target_os = "macos"))]
                {
                    window.close().unwrap();
                }
                #[cfg(target_os = "macos")]
                {
                    tauri::AppHandle::hide(&handle).unwrap();
                    window.close().unwrap();
                }
            }
            None => {}
        }
    }
}

pub fn set_window_always_on_top() -> bool {
    let handle = APP_HANDLE.get().unwrap();
    if let Some(window) = handle.get_webview_window("main") {
        let always_on_top = ALWAYS_ON_TOP.load(Ordering::Acquire);

        if !always_on_top {
            window.set_always_on_top(true).unwrap();
            ALWAYS_ON_TOP.store(true, Ordering::Release);
        } else {
            window.set_always_on_top(false).unwrap();
            ALWAYS_ON_TOP.store(false, Ordering::Release);
        }
        ALWAYS_ON_TOP.load(Ordering::Acquire)
    } else {
        false
    }
}

pub fn build_window<'a, R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: tauri::WebviewWindowBuilder<'a, R, M>,
) -> tauri::WebviewWindow<R> {
    #[cfg(target_os = "macos")]
    {
        let window = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .transparent(true)
            .build()
            .unwrap();

        post_process_window(&window);

        window
    }

    #[cfg(not(target_os = "macos"))]
    {
        let window = builder.transparent(true).decorations(true).build().unwrap();

        post_process_window(&window);

        window
    }
}

pub fn post_process_window<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    window.set_visible_on_all_workspaces(true).unwrap();

    let _ = window.current_monitor();

    #[cfg(target_os = "macos")]
    {
        use cocoa::appkit::NSWindowCollectionBehavior;
        use cocoa::base::id;

        let ns_win = window.ns_window().unwrap() as id;

        unsafe {
            // Disable the automatic creation of "Show Tab Bar" etc menu items on macOS
            NSWindow::setAllowsAutomaticWindowTabbing_(ns_win, cocoa::base::NO);

            let mut collection_behavior = ns_win.collectionBehavior();
            collection_behavior |=
                NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces;

            ns_win.setCollectionBehavior_(collection_behavior);
        }
    }
}

pub fn show_window(center: bool, to_mouse_position: bool, set_focus: bool) -> tauri::WebviewWindow {
    let window = get_window(center, to_mouse_position, set_focus);
    window.show().unwrap();
    window
}

pub fn get_window(center: bool, to_mouse_position: bool, set_focus: bool) -> tauri::WebviewWindow {
    let current_monitor = get_current_monitor();
    let handle = APP_HANDLE.get().unwrap();
    let window = match handle.get_webview_window("main") {
        Some(window) => {
            window.unminimize().unwrap();
            if set_focus {
                window.set_focus().unwrap();
            }
            window
        }
        None => {
            let config = config::get_config_by_app(handle).unwrap();

            let builder = tauri::WebviewWindowBuilder::new(
                handle,
                "main",
                tauri::WebviewUrl::App("/index.html".into()),
            )
            .title("Recorder")
            .fullscreen(false)
            .inner_size(620.0, 700.0)
            .min_inner_size(540.0, 600.0)
            .resizable(true)
            .skip_taskbar(config.hide_the_icon_in_the_dock.unwrap_or(true))
            .visible(false)
            .focused(false);

            build_window(builder)
        }
    };

    if to_mouse_position {
        debug_println!("Setting position to mouse position");
        let (mouse_logical_x, mouse_logical_y): (i32, i32) = get_mouse_location().unwrap();
        let window_physical_size = window.outer_size().unwrap();
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let mut mouse_physical_position = PhysicalPosition::new(mouse_logical_x, mouse_logical_y);
        if cfg!(target_os = "macos") {
            mouse_physical_position =
                LogicalPosition::new(mouse_logical_x as f64, mouse_logical_y as f64)
                    .to_physical(scale_factor);
        }

        let monitor_physical_size = current_monitor.size();
        let monitor_physical_position = current_monitor.position();

        let mut window_physical_position = mouse_physical_position;
        if mouse_physical_position.x + (window_physical_size.width as i32)
            > monitor_physical_position.x + (monitor_physical_size.width as i32)
        {
            window_physical_position.x = monitor_physical_position.x
                + (monitor_physical_size.width as i32)
                - (window_physical_size.width as i32);
        }
        if mouse_physical_position.y + (window_physical_size.height as i32)
            > monitor_physical_position.y + (monitor_physical_size.height as i32)
        {
            window_physical_position.y = monitor_physical_position.y
                + (monitor_physical_size.height as i32)
                - (window_physical_size.height as i32);
        }
        if !cfg!(target_os = "macos") {
            window.unminimize().unwrap();
        }
        debug_println!("Mouse physical position: {:?}", mouse_physical_position);
        debug_println!("Monitor physical size: {:?}", monitor_physical_size);
        debug_println!("Monitor physical position: {:?}", monitor_physical_position);
        debug_println!("Window physical size: {:?}", window_physical_size);
        debug_println!("Window physical position: {:?}", window_physical_position);
        window.set_position(window_physical_position).unwrap();
    } else if center {
        if !cfg!(target_os = "macos") {
            window.unminimize().unwrap();
        }
        window.center().unwrap();
    }

    #[cfg(debug_assertions)]
    window.open_devtools();

    window
}

pub fn get_mouse_location() -> Result<(i32, i32), String> {
    let position = Mouse::get_mouse_position();
    match position {
        Mouse::Position { x, y } => Ok((x, y)),
        Mouse::Error => Err("Error getting mouse position".to_string()),
    }
}

pub fn get_current_monitor() -> tauri::Monitor {
    let window = get_dummy_window();
    let (mouse_logical_x, mouse_logical_y): (i32, i32) = get_mouse_location().unwrap();
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let mut mouse_physical_position = PhysicalPosition::new(mouse_logical_x, mouse_logical_y);
    if cfg!(target_os = "macos") {
        mouse_physical_position =
            LogicalPosition::new(mouse_logical_x as f64, mouse_logical_y as f64)
                .to_physical(scale_factor);
    }
    window
        .available_monitors()
        .map(|monitors| {
            monitors
                .iter()
                .find(|monitor| {
                    let monitor_physical_size = monitor.size();
                    let monitor_physical_position = monitor.position();
                    mouse_physical_position.x >= monitor_physical_position.x
                        && mouse_physical_position.x
                            <= monitor_physical_position.x + (monitor_physical_size.width as i32)
                        && mouse_physical_position.y >= monitor_physical_position.y
                        && mouse_physical_position.y
                            <= monitor_physical_position.y + (monitor_physical_size.height as i32)
                })
                .cloned()
        })
        .unwrap_or_else(|e| {
            eprintln!("Error get available monitors: {}", e);
            None
        })
        .or_else(|| window.current_monitor().unwrap())
        .or_else(|| window.primary_monitor().unwrap())
        .expect("No current monitor found")
}

fn get_dummy_window() -> tauri::WebviewWindow {
    let app_handle = APP_HANDLE.get().unwrap();
    match app_handle.get_webview_window("dummy") {
        Some(window) => {
            debug_println!("Dummy window found!");
            window
        }
        None => {
            debug_println!("Create dummy window!");
            tauri::WebviewWindowBuilder::new(
                app_handle,
                "dummy",
                tauri::WebviewUrl::App("src/dummy.html".into()),
            )
            .title("Dummy")
            .visible(false)
            .build()
            .unwrap()
        }
    }
}
