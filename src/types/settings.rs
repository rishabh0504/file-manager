//! User settings and preferences.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    // Explorer settings
    pub hide_system_files: bool,
    pub show_hidden_files: bool,
    pub default_sort: String,
    pub confirm_before_delete: bool,

    // Operation settings
    pub auto_move_duplicates: bool,
    pub preserve_folder_structure: bool,
    pub enable_operation_history: bool,

    // UI settings
    pub theme: ThemePreference,
    pub show_thumbnails: bool,
    pub compact_mode: bool,

    // History settings
    pub operation_history_size: usize,
    pub keep_history_days: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ThemePreference {
    Light,
    Dark,
    Auto,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hide_system_files: true,
            show_hidden_files: false,
            default_sort: "name".to_string(),
            confirm_before_delete: true,

            auto_move_duplicates: false,
            preserve_folder_structure: true,
            enable_operation_history: true,

            theme: ThemePreference::Auto,
            show_thumbnails: true,
            compact_mode: false,

            operation_history_size: 500,
            keep_history_days: 30,
        }
    }
}

impl AppSettings {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("sortwise");

        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)?;
        }

        let config_file = config_dir.join("settings.json");

        if config_file.exists() {
            let json = std::fs::read_to_string(&config_file)?;
            Ok(serde_json::from_str(&json)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("sortwise");

        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)?;
        }

        let config_file = config_dir.join("settings.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_file, json)?;
        Ok(())
    }
}
