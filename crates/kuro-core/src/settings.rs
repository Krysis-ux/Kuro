
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::hardware::HardwareInfo;
use crate::tools::web_search::SearchProvider;
use crate::tools::ToolGroup;
use crate::Result;

pub const KEY_CONTEXT_SIZE: &str = "engine.contextSize";
pub const KEY_GPU_LAYERS: &str = "engine.gpuLayers";
pub const KEY_THREADS: &str = "engine.threads";
pub const KEY_IDLE_UNLOAD_MINUTES: &str = "engine.idleUnloadMinutes";
pub const KEY_ENGINE_RELEASE_TAG: &str = "engine.releaseTag";
pub const KEY_DEFAULT_MODEL: &str = "chat.defaultModel";
pub const KEY_SEARCH_PROVIDER: &str = "search.provider";
pub const KEY_SEARCH_BASE_URL: &str = "search.baseUrl";
pub const KEY_DEFAULT_TOOL_GROUPS: &str = "tools.defaultGroups";
pub const KEY_MEMORY_PRELOAD: &str = "tools.memoryPreload";
pub const KEY_MEMORY_ABOUT_YOU: &str = "memory.aboutYou";

pub fn about_you(db: &Db) -> Result<Option<String>> {
    Ok(db
        .get_setting(KEY_MEMORY_ABOUT_YOU)?
        .and_then(|value| value.as_str().map(str::to_string))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty()))
}

pub const KEY_CHAT_AUTO_ORCHESTRATE: &str = "chat.autoOrchestrate";
pub const KEY_CODE_AUTO_ORCHESTRATE: &str = "code.autoOrchestrate";
pub const KEY_CHAT_DEFAULT_EFFORT: &str = "chat.defaultEffort";
pub const KEY_CODE_DEFAULT_EFFORT: &str = "code.defaultEffort";
pub const KEY_CODE_DEFAULT_MODE: &str = "code.defaultMode";
pub const KEY_MODELS_DIRECTORY: &str = "models.directory";
pub const KEY_FREE_ALLOW_KEYLESS: &str = "free.allowKeyless";
pub const KEY_FREE_ALLOWANCES: &str = "free.allowances";

pub fn allow_keyless(db: &Db) -> Result<bool> {
    Ok(db
        .get_setting(KEY_FREE_ALLOW_KEYLESS)?
        .and_then(|value| value.as_bool())
        .unwrap_or(true))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allowance {
    #[serde(rename = "tokensPerDay", skip_serializing_if = "Option::is_none")]
    pub tokens_per_day: Option<i64>,
    #[serde(rename = "tokensPerMonth", skip_serializing_if = "Option::is_none")]
    pub tokens_per_month: Option<i64>,
}

impl Allowance {
    pub fn is_empty(self) -> bool {
        self.tokens_per_day.is_none() && self.tokens_per_month.is_none()
    }
}

pub fn free_allowances(db: &Db) -> Result<HashMap<String, Allowance>> {
    let Some(value) = db.get_setting(KEY_FREE_ALLOWANCES)? else {
        return Ok(HashMap::new());
    };
    Ok(serde_json::from_value(value).unwrap_or_default())
}

pub fn set_free_allowance(db: &Db, slug: &str, allowance: Option<Allowance>) -> Result<()> {
    let mut held = free_allowances(db)?;
    match allowance.filter(|entry| !entry.is_empty()) {
        Some(entry) => held.insert(slug.to_string(), entry),
        None => held.remove(slug),
    };
    db.set_setting(KEY_FREE_ALLOWANCES, &serde_json::to_value(held)?)
}

pub fn auto_orchestrate(db: &Db, surface: crate::orchestrate::Surface) -> Result<bool> {
    let key = match surface {
        crate::orchestrate::Surface::Chat => KEY_CHAT_AUTO_ORCHESTRATE,
        crate::orchestrate::Surface::Code => KEY_CODE_AUTO_ORCHESTRATE,
    };
    Ok(db.get_setting(key)?.and_then(|value| value.as_bool()).unwrap_or(true))
}

pub fn default_effort(db: &Db, surface: crate::orchestrate::Surface) -> Result<Effort> {
    let (key, fallback) = match surface {
        crate::orchestrate::Surface::Chat => (KEY_CHAT_DEFAULT_EFFORT, Effort::Balanced),
        crate::orchestrate::Surface::Code => (KEY_CODE_DEFAULT_EFFORT, Effort::High),
    };

    Ok(db
        .get_setting(key)?
        .and_then(|value| value.as_str().and_then(Effort::parse))
        .unwrap_or(fallback))
}

pub fn default_workspace_mode(db: &Db) -> Result<crate::workspace::WorkspaceMode> {
    Ok(db
        .get_setting(KEY_CODE_DEFAULT_MODE)?
        .and_then(|value| {
            value
                .as_str()
                .and_then(crate::workspace::WorkspaceMode::parse)
        })
        .unwrap_or(crate::workspace::WorkspaceMode::Agent))
}

const DEFAULT_IDLE_UNLOAD_MINUTES: u32 = 30;

#[derive(Debug, Clone, Serialize)]
pub struct EngineSettings {
    pub context_size: u32,
    pub gpu_layers: i32,
    pub threads: u32,
    pub idle_unload_minutes: u32,
}

impl EngineSettings {
    pub fn resolve(db: &Db, hardware: &HardwareInfo) -> Result<Self> {
        let context_size = read_u32(db, KEY_CONTEXT_SIZE)?
            .filter(|value| *value > 0)
            .unwrap_or(hardware.recommended.context_size);

        let gpu_layers = read_i64(db, KEY_GPU_LAYERS)?
            .filter(|value| *value >= 0)
            .map(|value| value as i32)
            .unwrap_or(hardware.recommended.gpu_layers);

        let threads = read_u32(db, KEY_THREADS)?
            .filter(|value| *value > 0)
            .unwrap_or(hardware.recommended.threads);

        let idle_unload_minutes =
            read_u32(db, KEY_IDLE_UNLOAD_MINUTES)?.unwrap_or(DEFAULT_IDLE_UNLOAD_MINUTES);

        Ok(Self {
            context_size,
            gpu_layers,
            threads,
            idle_unload_minutes,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SearchSettings {
    pub provider: SearchProvider,
    pub base_url: Option<String>,
}

impl SearchSettings {
    pub const KEY_REFERENCE: &'static str = "search:api-key";

    pub fn resolve(db: &Db) -> Result<Self> {
        let provider = db
            .get_setting(KEY_SEARCH_PROVIDER)?
            .and_then(|value| value.as_str().and_then(SearchProvider::parse))
            .unwrap_or(SearchProvider::Duckduckgo);

        let base_url = db
            .get_setting(KEY_SEARCH_BASE_URL)?
            .and_then(|value| value.as_str().map(str::to_string))
            .filter(|url| !url.trim().is_empty());

        Ok(Self { provider, base_url })
    }
}

pub fn default_tool_groups(db: &Db) -> Result<Vec<ToolGroup>> {
    let fallback = || vec![ToolGroup::Memory, ToolGroup::Projects];

    let Some(stored) = db.get_setting(KEY_DEFAULT_TOOL_GROUPS)? else {
        return Ok(fallback());
    };

    Ok(stored
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .filter_map(ToolGroup::parse)
                .collect()
        })
        .unwrap_or_else(fallback))
}

pub fn memory_preload_enabled(db: &Db) -> Result<bool> {
    Ok(db
        .get_setting(KEY_MEMORY_PRELOAD)?
        .and_then(|value| value.as_bool())
        .unwrap_or(true))
}

fn read_u32(db: &Db, key: &str) -> Result<Option<u32>> {
    Ok(db
        .get_setting(key)?
        .and_then(|value| value.as_u64())
        .map(|value| value as u32))
}

fn read_i64(db: &Db, key: &str) -> Result<Option<i64>> {
    Ok(db.get_setting(key)?.and_then(|value| value.as_i64()))
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    #[default]
    Balanced,
    High,
    Max,
    Ultra,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct EffortParams {
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
    pub reasoning_effort: Option<&'static str>,
}

impl Effort {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "low" | "instant" => Some(Self::Low),
            "balanced" | "medium" => Some(Self::Balanced),
            "high" | "thinking" => Some(Self::High),
            "max" | "extended" => Some(Self::Max),
            "ultra" | "ultracode" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Balanced => "balanced",
            Self::High => "high",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }

    pub fn coding_only(self) -> bool {
        self == Self::Ultra
    }

    pub fn ceiling(coding: bool) -> Self {
        if coding {
            Self::Ultra
        } else {
            Self::Max
        }
    }

    pub fn params(self) -> EffortParams {
        match self {
            Self::Low => EffortParams {
                temperature: 0.5,
                top_p: 0.9,
                max_tokens: 512,
                reasoning_effort: Some("low"),
            },
            Self::Balanced => EffortParams {
                temperature: 0.7,
                top_p: 0.95,
                max_tokens: 2048,
                reasoning_effort: Some("medium"),
            },
            Self::High => EffortParams {
                temperature: 0.7,
                top_p: 0.95,
                max_tokens: 4096,
                reasoning_effort: Some("high"),
            },
            Self::Max => EffortParams {
                temperature: 0.8,
                top_p: 0.97,
                max_tokens: 8192,
                reasoning_effort: Some("high"),
            },
            Self::Ultra => EffortParams {
                temperature: 0.7,
                top_p: 0.95,
                max_tokens: 16384,
                reasoning_effort: Some("high"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware;
    use serde_json::json;

    #[test]
    fn falls_back_to_hardware_defaults_when_unset() {
        let db = Db::open_in_memory().expect("open");
        let hw = hardware::detect();

        let settings = EngineSettings::resolve(&db, &hw).expect("resolve");

        assert_eq!(settings.context_size, hw.recommended.context_size);
        assert_eq!(settings.threads, hw.recommended.threads);
        assert_eq!(settings.idle_unload_minutes, DEFAULT_IDLE_UNLOAD_MINUTES);
    }

    #[test]
    fn user_overrides_win_over_defaults() {
        let db = Db::open_in_memory().expect("open");
        let hw = hardware::detect();
        db.set_setting(KEY_CONTEXT_SIZE, &json!(16384)).expect("set");
        db.set_setting(KEY_THREADS, &json!(4)).expect("set");

        let settings = EngineSettings::resolve(&db, &hw).expect("resolve");

        assert_eq!(settings.context_size, 16384);
        assert_eq!(settings.threads, 4);
    }

    #[test]
    fn zero_means_auto_not_zero() {
        let db = Db::open_in_memory().expect("open");
        let hw = hardware::detect();
        db.set_setting(KEY_CONTEXT_SIZE, &json!(0)).expect("set");
        db.set_setting(KEY_THREADS, &json!(0)).expect("set");

        let settings = EngineSettings::resolve(&db, &hw).expect("resolve");

        assert_eq!(settings.context_size, hw.recommended.context_size);
        assert_eq!(settings.threads, hw.recommended.threads);
    }

    #[test]
    fn idle_unload_can_be_disabled() {
        let db = Db::open_in_memory().expect("open");
        let hw = hardware::detect();
        db.set_setting(KEY_IDLE_UNLOAD_MINUTES, &json!(0)).expect("set");

        let settings = EngineSettings::resolve(&db, &hw).expect("resolve");

        assert_eq!(settings.idle_unload_minutes, 0, "0 must mean never unload");
    }

    #[test]
    fn effort_increases_the_token_budget_monotonically() {
        let budgets: Vec<u32> = [
            Effort::Low,
            Effort::Balanced,
            Effort::High,
            Effort::Max,
            Effort::Ultra,
        ]
        .iter()
        .map(|effort| effort.params().max_tokens)
        .collect();

        assert!(budgets.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(Effort::default(), Effort::Balanced);
        assert_eq!(Effort::parse("HIGH"), Some(Effort::High));
        assert_eq!(Effort::parse("nonsense"), None);
    }

    #[test]
    fn ultra_is_offered_when_coding_and_nowhere_else() {
        assert_eq!(Effort::ceiling(true), Effort::Ultra);
        assert_eq!(Effort::ceiling(false), Effort::Max);
        assert!(Effort::Ultra.coding_only());
        assert!(!Effort::Max.coding_only());
    }

    #[test]
    fn ultra_buys_persistence_rather_than_randomness() {
        let max = Effort::Max.params();
        let ultra = Effort::Ultra.params();

        assert!(ultra.max_tokens > max.max_tokens);
        assert!(ultra.temperature <= max.temperature);
    }

    #[test]
    fn the_names_other_coding_tools_use_are_accepted() {
        assert_eq!(Effort::parse("ultracode"), Some(Effort::Ultra));
        assert_eq!(Effort::parse("medium"), Some(Effort::Balanced));
        assert_eq!(Effort::parse("instant"), Some(Effort::Low));
    }

    #[test]
    fn reading_and_writing_your_own_description_round_trips() {
        let db = Db::open_in_memory().expect("open");
        assert!(about_you(&db).expect("read").is_none());

        db.set_setting(KEY_MEMORY_ABOUT_YOU, &json!("  I work in Rust.  "))
            .expect("set");
        assert_eq!(about_you(&db).expect("read").as_deref(), Some("I work in Rust."));

        db.set_setting(KEY_MEMORY_ABOUT_YOU, &json!("   ")).expect("set");
        assert!(about_you(&db).expect("read").is_none());
    }

    #[test]
    fn a_fresh_install_reads_memory_and_projects_but_does_not_search() {
        let db = Db::open_in_memory().expect("open");
        let groups = default_tool_groups(&db).expect("groups");

        assert!(groups.contains(&ToolGroup::Memory));
        assert!(groups.contains(&ToolGroup::Projects));
        assert!(
            !groups.contains(&ToolGroup::Web),
            "searching is the moment a question leaves the machine, so it is opted into"
        );
    }
}
