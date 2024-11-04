use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{path::BaseDirectory, AppHandle, Manager};

use crate::APP_HANDLE;
use debug_print::debug_println;

pub const CONFIG_PATH: &str = "recorder.wcw.apps.wcw-recorder";
static CONFIG_CACHE: Mutex<Option<Config>> = Mutex::new(None);

#[tauri::command]
#[specta::specta]
pub fn clear_config_cache() {
    CONFIG_CACHE.lock().take();
}

#[tauri::command]
#[specta::specta]
pub fn get_config_content() -> String {
    if let Some(app) = APP_HANDLE.get() {
        return get_config_content_by_app(app).unwrap();
    } else {
        return "{}".to_string();
    }
}

#[tauri::command]
#[specta::specta]
pub fn update_config(config_content: &str) {
    if let Some(app) = APP_HANDLE.get() {
        let old = get_config_by_app(app).unwrap();
        let config: Config = serde_json::from_str(&config_content).unwrap();
        write_config(app, merge_config(config, old));
        // TODO: event to release ui
        // Ok("update success.")
    } else {
        println!("fail to get app handle.");
        // Err("fail to update config.")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub hotkey: Option<String>,
    pub display_window_hotkey: Option<String>,
    pub writing_hotkey: Option<String>,
    pub always_show_icons: Option<bool>,
    pub hide_the_icon_in_the_dock: Option<bool>,
    pub configured: Option<bool>,
}

pub fn get_config() -> Result<Config, Box<dyn std::error::Error>> {
    let app_handle = APP_HANDLE.get().unwrap();
    get_config_by_app(app_handle)
}

pub fn get_config_by_app(app: &AppHandle) -> Result<Config, Box<dyn std::error::Error>> {
    let conf = _get_config_by_app(app);
    match conf {
        Ok(conf) => Ok(conf),
        Err(e) => {
            println!("get config failed: {}", e);
            Err(e)
        }
    }
}

pub fn _get_config_by_app(app: &AppHandle) -> Result<Config, Box<dyn std::error::Error>> {
    if let Some(config_cache) = &*CONFIG_CACHE.lock() {
        return Ok(config_cache.clone());
    }
    let config_content = get_config_content_by_app(app)?;
    let config: Config = serde_json::from_str(&config_content)?;
    CONFIG_CACHE.lock().replace(config.clone());
    Ok(config)
}

pub fn get_config_content_by_app(app: &AppHandle) -> Result<String, String> {
    let app_config_dir = app
        .path()
        .resolve("recorder.wcw.apps.wcw-recorder", BaseDirectory::Config)
        .unwrap();
    if !app_config_dir.exists() {
        std::fs::create_dir_all(&app_config_dir).unwrap();
    }
    let config_path = app_config_dir.join("config.json");

    debug_println!(
        "get config from path: {}",
        config_path.as_os_str().to_str().unwrap()
    );
    if config_path.exists() {
        match std::fs::read_to_string(config_path) {
            Ok(content) => Ok(content),
            Err(_) => Err("Failed to read config file".to_string()),
        }
    } else {
        std::fs::write(config_path, "{}").unwrap();
        Ok("{}".to_string())
    }
}

pub fn merge_config(cfg: Config, old: Config) -> Config {
    Config {
        configured: cfg.configured.or(old.configured),
        hotkey: cfg.hotkey.or(old.hotkey),
        display_window_hotkey: cfg.display_window_hotkey.or(old.display_window_hotkey),
        writing_hotkey: cfg.writing_hotkey.or(old.writing_hotkey),
        always_show_icons: cfg.always_show_icons.or(old.always_show_icons),
        hide_the_icon_in_the_dock: cfg
            .hide_the_icon_in_the_dock
            .or(old.hide_the_icon_in_the_dock),
    }
}

pub fn write_config(app: &AppHandle, config: Config) {
    let app_config_dir = app
        .path()
        .resolve(CONFIG_PATH, BaseDirectory::Config)
        .unwrap();
    if !app_config_dir.exists() {
        std::fs::create_dir_all(&app_config_dir).unwrap();
    }

    let config_path = app_config_dir.join("config.json");
    let content = serde_json::to_string(&config).unwrap();
    std::fs::write(config_path, content).unwrap();
}
