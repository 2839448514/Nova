use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn default_custom_models() -> HashMap<String, Vec<String>> {
    HashMap::new()
}

fn default_provider_profiles() -> HashMap<String, ProviderProfile> {
    HashMap::new()
}

fn normalize_provider_key(provider: &str) -> String {
    let key = provider.trim().to_ascii_lowercase();
    if key.is_empty() {
        "anthropic".to_string()
    } else {
        key
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub provider: String,
    #[serde(default = "default_custom_models")]
    pub custom_models: HashMap<String, Vec<String>>,
    #[serde(default = "default_provider_profiles")]
    pub provider_profiles: HashMap<String, ProviderProfile>,
    #[serde(default)]
    pub disabled_skills: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_key: "".to_string(),
            base_url: "".to_string(),
            model: "".to_string(),
            provider: "anthropic".to_string(),
            custom_models: HashMap::new(),
            provider_profiles: HashMap::new(),
            disabled_skills: Vec::new(),
        }
    }
}

impl AppSettings {
    pub fn active_provider_key(&self) -> String {
        normalize_provider_key(&self.provider)
    }

    pub fn active_provider_profile(&self) -> ProviderProfile {
        let key = self.active_provider_key();
        if let Some(profile) = self.provider_profiles.get(&key) {
            let mut merged = profile.clone();
            if merged.api_key.is_empty() {
                merged.api_key = self.api_key.clone();
            }
            if merged.base_url.is_empty() {
                merged.base_url = self.base_url.clone();
            }
            if merged.model.is_empty() {
                merged.model = self.model.clone();
            }
            return merged;
        }

        ProviderProfile {
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
        }
    }

    pub fn normalize_for_runtime(&mut self) {
        let key = self.active_provider_key();
        self.provider = key.clone();

        let has_legacy = !self.api_key.is_empty() || !self.base_url.is_empty() || !self.model.is_empty();
        if has_legacy && !self.provider_profiles.contains_key(&key) {
            self.provider_profiles.insert(
                key.clone(),
                ProviderProfile {
                    api_key: self.api_key.clone(),
                    base_url: self.base_url.clone(),
                    model: self.model.clone(),
                },
            );
        }

        if let Some(active) = self.provider_profiles.get(&key) {
            if !active.api_key.is_empty() {
                self.api_key = active.api_key.clone();
            }
            if !active.base_url.is_empty() {
                self.base_url = active.base_url.clone();
            }
            if !active.model.is_empty() {
                self.model = active.model.clone();
            }
        }
    }
}

pub fn get_settings_path(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap().join("settings.json")
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppSettings {
    let path = get_settings_path(&app);
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(mut settings) = serde_json::from_str::<AppSettings>(&content) {
                settings.normalize_for_runtime();
                return settings;
            }
        }
    }
    let mut settings = AppSettings::default();
    settings.normalize_for_runtime();
    settings
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = get_settings_path(&app);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(e.to_string());
        }
    }
    let mut normalized = settings;
    normalized.normalize_for_runtime();
    let content = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}
