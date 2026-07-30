//! Typed access to the settings the backend itself acts on.
//!
//! The `settings` table is otherwise a free-form JSON key/value store that the
//! frontend owns (display toggles, composer preferences and so on). Only the
//! keys the Rust side needs to make decisions about are given typed accessors
//! here, so adding a UI-only preference never requires a backend change.

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::hardware::HardwareInfo;
use crate::Result;

pub const KEY_CONTEXT_SIZE: &str = "engine.contextSize";
pub const KEY_GPU_LAYERS: &str = "engine.gpuLayers";
pub const KEY_THREADS: &str = "engine.threads";
pub const KEY_IDLE_UNLOAD_MINUTES: &str = "engine.idleUnloadMinutes";
pub const KEY_ENGINE_RELEASE_TAG: &str = "engine.releaseTag";
pub const KEY_DEFAULT_MODEL: &str = "chat.defaultModel";

/// How long a model stays resident with no requests before it is unloaded.
/// Zero means "keep it loaded until told otherwise".
const DEFAULT_IDLE_UNLOAD_MINUTES: u32 = 30;

/// Engine options resolved against the user's overrides and this machine's
/// capabilities. A stored value of `0` (or `-1` for GPU layers) means "Auto",
/// which is what the Settings UI shows by default.
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

fn read_u32(db: &Db, key: &str) -> Result<Option<u32>> {
    Ok(db
        .get_setting(key)?
        .and_then(|value| value.as_u64())
        .map(|value| value as u32))
}

fn read_i64(db: &Db, key: &str) -> Result<Option<i64>> {
    Ok(db.get_setting(key)?.and_then(|value| value.as_i64()))
}

/// How much compute to spend on a reply.
///
/// This is the single control shown in the chat composer. It bundles the
/// parameters a beginner should not have to reason about; the raw sampling
/// knobs still live in Settings for power users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    #[default]
    Balanced,
    High,
    Max,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct EffortParams {
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
    /// Passed through to models that expose a thinking budget. Models that do
    /// not understand it simply ignore the field.
    pub reasoning_effort: Option<&'static str>,
}

impl Effort {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "balanced" => Some(Self::Balanced),
            "high" => Some(Self::High),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Balanced => "balanced",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    /// Effort mainly buys output length and thinking budget. Temperature moves
    /// only slightly, because "try harder" should not mean "be more random".
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
        let budgets: Vec<u32> = [Effort::Low, Effort::Balanced, Effort::High, Effort::Max]
            .iter()
            .map(|effort| effort.params().max_tokens)
            .collect();

        assert!(budgets.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(Effort::default(), Effort::Balanced);
        assert_eq!(Effort::parse("HIGH"), Some(Effort::High));
        assert_eq!(Effort::parse("nonsense"), None);
    }
}
