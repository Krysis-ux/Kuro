//! Typed access to the settings the backend itself acts on.
//!
//! The `settings` table is otherwise a free-form JSON key/value store that the
//! frontend owns (display toggles, composer preferences and so on). Only the
//! keys the Rust side needs to make decisions about are given typed accessors
//! here, so adding a UI-only preference never requires a backend change.

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
/// Tool groups on by default in a new conversation.
pub const KEY_DEFAULT_TOOL_GROUPS: &str = "tools.defaultGroups";
/// Whether saved memories are prepended to a conversation without being asked for.
pub const KEY_MEMORY_PRELOAD: &str = "tools.memoryPreload";
/// Standing facts the user typed about themselves, rather than ones a model saved.
pub const KEY_MEMORY_ABOUT_YOU: &str = "memory.aboutYou";

/// What the user wants every model to know about them.
///
/// Distinct from the memories a model saves with `remember`, and worth being
/// distinct: those accumulate from conversations and are edited by deleting
/// individual rows, whereas this is a paragraph somebody writes deliberately and
/// rewrites when it stops being true. Asking people to teach it to the model one
/// conversation at a time, when they already know what they want it to know, is
/// the long way round.
pub fn about_you(db: &Db) -> Result<Option<String>> {
    Ok(db
        .get_setting(KEY_MEMORY_ABOUT_YOU)?
        .and_then(|value| value.as_str().map(str::to_string))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty()))
}

/* --- Per-surface preferences ---
 *
 * Chat and coding are separate settings rather than one shared set, because the
 * right answer differs on almost every one of them. A chat that quietly started
 * running the tests would be alarming; a coding turn that did not would be
 * useless. Keeping them apart is what lets each have a sensible default instead
 * of a compromise that suits neither.
 */

/// Whether effort also selects skills and tool budget on the chat surface.
pub const KEY_CHAT_AUTO_ORCHESTRATE: &str = "chat.autoOrchestrate";
/// The same, for coding workspaces.
pub const KEY_CODE_AUTO_ORCHESTRATE: &str = "code.autoOrchestrate";
/// Effort a new chat starts at.
pub const KEY_CHAT_DEFAULT_EFFORT: &str = "chat.defaultEffort";
/// Effort a coding turn starts at.
pub const KEY_CODE_DEFAULT_EFFORT: &str = "code.defaultEffort";
/// Mode a newly opened workspace starts in.
pub const KEY_CODE_DEFAULT_MODE: &str = "code.defaultMode";
/// Where downloaded model files are kept. Empty means Kuro's own data directory.
pub const KEY_MODELS_DIRECTORY: &str = "models.directory";
/// Whether the free pool may fall back to shared, unauthenticated endpoints.
pub const KEY_FREE_ALLOW_KEYLESS: &str = "free.allowKeyless";
/// Per-provider allowances the user typed in, as a JSON object.
pub const KEY_FREE_ALLOWANCES: &str = "free.allowances";

/// Whether shared, keyless endpoints may answer.
///
/// On by default, which is what makes Kuro Free do something useful before the
/// user has signed up for anything. The cost is real and stated on the screen:
/// these are shared endpoints, rate-limited per address, and several log or
/// train on what they receive. They are always ranked below any provider the
/// user holds a key for, so turning this on can only add an answer where there
/// would otherwise have been an error.
pub fn allow_keyless(db: &Db) -> Result<bool> {
    Ok(db
        .get_setting(KEY_FREE_ALLOW_KEYLESS)?
        .and_then(|value| value.as_bool())
        .unwrap_or(true))
}

/// A ceiling the user typed in for one provider.
///
/// Optional and unverified. Providers state their allowances in half a dozen
/// incompatible units — requests per minute, tokens per day, neurons, dollars
/// of credit — and change them without notice, so Kuro asks rather than
/// guesses, and shows a bar only where somebody has answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allowance {
    #[serde(rename = "tokensPerDay", skip_serializing_if = "Option::is_none")]
    pub tokens_per_day: Option<i64>,
    #[serde(rename = "tokensPerMonth", skip_serializing_if = "Option::is_none")]
    pub tokens_per_month: Option<i64>,
}

impl Allowance {
    /// Whether this says anything at all.
    pub fn is_empty(self) -> bool {
        self.tokens_per_day.is_none() && self.tokens_per_month.is_none()
    }
}

/// Every allowance the user has entered, by provider slug.
pub fn free_allowances(db: &Db) -> Result<HashMap<String, Allowance>> {
    let Some(value) = db.get_setting(KEY_FREE_ALLOWANCES)? else {
        return Ok(HashMap::new());
    };
    // A stored value Kuro cannot read is treated as none rather than as an
    // error: this is optional decoration on a screen, and failing the whole
    // free-models page over it would take away the place it is edited.
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// Set or clear one provider's allowance.
pub fn set_free_allowance(db: &Db, slug: &str, allowance: Option<Allowance>) -> Result<()> {
    let mut held = free_allowances(db)?;
    match allowance.filter(|entry| !entry.is_empty()) {
        Some(entry) => held.insert(slug.to_string(), entry),
        None => held.remove(slug),
    };
    db.set_setting(KEY_FREE_ALLOWANCES, &serde_json::to_value(held)?)
}

/// Whether effort should also pick skills and a tool budget.
///
/// On by default on both surfaces. The alternative — an effort dial that only
/// moves a temperature — is the state this replaced, and it made the control
/// look decorative because on most turns it was.
pub fn auto_orchestrate(db: &Db, surface: crate::orchestrate::Surface) -> Result<bool> {
    let key = match surface {
        crate::orchestrate::Surface::Chat => KEY_CHAT_AUTO_ORCHESTRATE,
        crate::orchestrate::Surface::Code => KEY_CODE_AUTO_ORCHESTRATE,
    };
    Ok(db.get_setting(key)?.and_then(|value| value.as_bool()).unwrap_or(true))
}

/// The effort a surface starts at.
///
/// Coding defaults higher than chat. A coding turn's first few rounds are spent
/// reading the project rather than answering, so the setting that looks
/// extravagant in a chat is merely adequate in a workspace.
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

/// The mode a newly opened workspace starts in.
///
/// Agent, because that is the mode the Code page is built around: every change
/// it makes is recorded with the previous contents and is one click from being
/// undone, which is the whole reason that panel exists. Starting in Plan would
/// mean the common case — "change this for me" — is answered by a description of
/// a change, and the user's first action in a new workspace is always to switch
/// the mode. Bypass is never a default; it is the only one that turns a
/// protection off rather than on.
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

/// Web search preferences.
///
/// The API key is deliberately absent: it lives in the credential store, and the
/// settings endpoint is readable by the browser. Keeping the two apart means the
/// UI can show which provider is selected without ever being able to read its key.
#[derive(Debug, Clone)]
pub struct SearchSettings {
    pub provider: SearchProvider,
    pub base_url: Option<String>,
}

impl SearchSettings {
    /// Credential-store reference for the search provider's key. One reference
    /// rather than one per provider, because only one provider is active at a time
    /// and a stale key for an unselected provider is a liability, not a feature.
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

/// Tool groups a new conversation starts with.
///
/// Memory is on by default and web is not. Memory only ever reads what the user
/// themselves asked to be saved, whereas search sends the query off the machine —
/// which is exactly the thing this application promises not to do unprompted.
pub fn default_tool_groups(db: &Db) -> Result<Vec<ToolGroup>> {
    // Memory and project reading, but not web. Both of the defaults only ever
    // touch things the user themselves put here — facts they asked to be saved,
    // folders they opened on the Code page — whereas search sends the query off
    // the machine, which is exactly the thing this application promises not to
    // do unprompted.
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

/// Whether saved memories are put in front of the model unasked.
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

/// How much compute to spend on a reply.
///
/// This is the single control shown in the chat composer. It bundles the
/// parameters a beginner should not have to reason about; the raw sampling
/// knobs still live in Settings for power users.
/// Declaration order is the ordering: `Low < Balanced < High < Max`. Several
/// callers ask "is this at least High", and a hand-written comparison would be
/// one more thing to keep in step with this list.
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
    /// Everything, and only offered when coding.
    ///
    /// A chat has nothing to spend this on: there is no project to read, no
    /// build to run, and past a point more rounds of web search is just a slower
    /// wrong answer. A coding turn does — so this is the level that stops
    /// rationing, turns on every skill the project justifies, and lets the tool
    /// loop run as long as it is allowed to.
    Ultra,
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

    /// Whether this level is only meaningful on the coding surface.
    pub fn coding_only(self) -> bool {
        self == Self::Ultra
    }

    /// The highest level a surface offers.
    pub fn ceiling(coding: bool) -> Self {
        if coding {
            Self::Ultra
        } else {
            Self::Max
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
            // Deliberately not hotter than Max. Ultra buys persistence — more
            // rounds, more tools, more expertise in the brief — and turning the
            // temperature up as well would trade the reliability that a long
            // agentic run depends on for variety nobody asked for.
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
        // A chat has nothing to spend it on, and offering a level that does
        // nothing there would make the whole control look arbitrary.
        assert_eq!(Effort::ceiling(true), Effort::Ultra);
        assert_eq!(Effort::ceiling(false), Effort::Max);
        assert!(Effort::Ultra.coding_only());
        assert!(!Effort::Max.coding_only());
    }

    #[test]
    fn ultra_buys_persistence_rather_than_randomness() {
        // Turning the temperature up as well would trade the reliability a long
        // agentic run depends on for variety nobody asked for.
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

        // A field somebody emptied is not a description of them.
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
