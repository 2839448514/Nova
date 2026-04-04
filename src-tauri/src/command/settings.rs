use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn default_custom_models() -> HashMap<String, Vec<String>> {
    // custom_models 默认空映射。
    HashMap::new()
}

fn default_provider_profiles() -> HashMap<String, ProviderProfile> {
    // provider_profiles 默认空映射。
    HashMap::new()
}

fn default_hook_env() -> HashMap<String, String> {
    // hook_env 默认空映射。
    HashMap::new()
}

const HOOK_ENV_KEYS: &[&str] = &[
    "NOVA_PRE_TOOL_DENY_TOOLS",
    "NOVA_PRE_TOOL_CONTEXT",
    "NOVA_POST_TOOL_CONTEXT",
    "NOVA_POST_TOOL_STOP_ON_ERROR",
    "NOVA_POST_TOOL_BLOCK_PATTERN",
    "NOVA_POST_TOOL_FAILURE_CONTEXT",
    "NOVA_POST_TOOL_FAILURE_STOP",
    "NOVA_STOP_HOOK_MAX_ASSISTANT_MESSAGES",
    "NOVA_STOP_HOOK_BLOCK_PATTERN",
    "NOVA_STOP_HOOK_APPEND_CONTEXT",
];

fn normalize_provider_key(provider: &str) -> String {
    // provider 名去空白并转小写。
    let key = provider.trim().to_ascii_lowercase();
    // 空 provider 回退 anthropic。
    if key.is_empty() {
        "anthropic".to_string()
    } else {
        // 返回规范化 provider key。
        key
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    #[serde(default)]
    // provider API key。
    pub api_key: String,
    #[serde(default)]
    // provider base_url。
    pub base_url: String,
    #[serde(default)]
    // provider model。
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    // 兼容旧字段：当前 API key。
    pub api_key: String,
    // 兼容旧字段：当前 base_url。
    pub base_url: String,
    // 兼容旧字段：当前 model。
    pub model: String,
    // 当前 provider 标识。
    pub provider: String,
    #[serde(default = "default_custom_models")]
    // 各 provider 的自定义模型列表。
    pub custom_models: HashMap<String, Vec<String>>,
    #[serde(default = "default_provider_profiles")]
    // 各 provider 的独立配置。
    pub provider_profiles: HashMap<String, ProviderProfile>,
    #[serde(default)]
    // 被禁用的技能列表。
    pub disabled_skills: Vec<String>,
    #[serde(default = "default_hook_env", alias = "hook_env")]
    // 钩子环境变量配置。
    pub hook_env: HashMap<String, String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        // 应用设置默认值。
        Self {
            api_key: "".to_string(),
            base_url: "".to_string(),
            model: "".to_string(),
            provider: "anthropic".to_string(),
            custom_models: HashMap::new(),
            provider_profiles: HashMap::new(),
            disabled_skills: Vec::new(),
            hook_env: HashMap::new(),
        }
    }
}

impl AppSettings {
    pub fn active_provider_key(&self) -> String {
        // 返回规范化后的当前 provider key。
        normalize_provider_key(&self.provider)
    }

    pub fn active_provider_profile(&self) -> ProviderProfile {
        // 计算当前 provider key。
        let key = self.active_provider_key();
        // 若存在 provider 专属配置，优先使用并与旧字段合并。
        if let Some(profile) = self.provider_profiles.get(&key) {
            // 克隆 provider profile 作为可变副本。
            let mut merged = profile.clone();
            // 专属 api_key 为空时回退旧字段。
            if merged.api_key.is_empty() {
                merged.api_key = self.api_key.clone();
            }
            // 专属 base_url 为空时回退旧字段。
            if merged.base_url.is_empty() {
                merged.base_url = self.base_url.clone();
            }
            // 专属 model 为空时回退旧字段。
            if merged.model.is_empty() {
                merged.model = self.model.clone();
            }
            // 返回合并后的 profile。
            return merged;
        }

        // 不存在专属配置时，直接由旧字段构造 profile。
        ProviderProfile {
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
        }
    }

    pub fn normalize_for_runtime(&mut self) {
        // 规范化 provider key。
        let key = self.active_provider_key();
        // 将 provider 字段回写为规范化值。
        self.provider = key.clone();

        // 检查旧字段是否有值。
        let has_legacy = !self.api_key.is_empty() || !self.base_url.is_empty() || !self.model.is_empty();
        // 若存在旧字段且缺失专属 profile，则自动迁入 provider_profiles。
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

        // 用当前 provider 的专属配置覆盖旧字段展示值。
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
    // 设置文件路径：app_data_dir/settings.json。
    app.path().app_data_dir().unwrap().join("settings.json")
}

fn sync_hook_env_vars(settings: &AppSettings) {
    for key in HOOK_ENV_KEYS {
        let value = settings
            .hook_env
            .get(*key)
            .map(|v| v.trim().to_string())
            .unwrap_or_default();

        if value.is_empty() {
            std::env::remove_var(key);
        } else {
            std::env::set_var(key, value);
        }
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppSettings {
    // 获取 settings.json 路径。
    let path = get_settings_path(&app);
    // 文件存在时尝试读取并反序列化。
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(mut settings) = serde_json::from_str::<AppSettings>(&content) {
                // 运行时规范化后返回。
                settings.normalize_for_runtime();
                sync_hook_env_vars(&settings);
                return settings;
            }
        }
    }
    // 读取失败时回退默认配置并规范化。
    let mut settings = AppSettings::default();
    settings.normalize_for_runtime();
    sync_hook_env_vars(&settings);
    settings
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    // 获取 settings.json 路径。
    let path = get_settings_path(&app);
    // 确保父目录存在。
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(e.to_string());
        }
    }
    // 对传入设置做运行时规范化。
    let mut normalized = settings;
    normalized.normalize_for_runtime();
    sync_hook_env_vars(&normalized);
    // 序列化为美化 JSON。
    let content = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    // 写入文件。
    std::fs::write(path, content).map_err(|e| e.to_string())
}
