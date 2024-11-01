use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{path::BaseDirectory, AppHandle};

use crate::APP_HANDLE;
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub hotkey: Option<String>,
    pub display_window_hotkey: Option<String>,
    pub writing_hotkey: Option<String>,
    pub always_show_icons: Option<bool>,
    pub hide_the_icon_in_the_dock: Option<bool>,
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
    Ok("{}".to_string())

    // let app_config_dir = app
    //     .path()
    //     .resolve("xyz.yetone.apps.openai-translator", BaseDirectory::Config)
    //     .unwrap();
    // if !app_config_dir.exists() {
    //     std::fs::create_dir_all(&app_config_dir).unwrap();
    // }
    // let config_path = app_config_dir.join("config.json");
    // if config_path.exists() {
    //     match std::fs::read_to_string(config_path) {
    //         Ok(content) => Ok(content),
    //         Err(_) => Err("Failed to read config file".to_string()),
    //     }
    // } else {
    //     std::fs::write(config_path, "{}").unwrap();
    //     Ok("{}".to_string())
    // }
}
