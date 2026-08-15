
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::classify::{classify, Speciality};
use crate::wire::{Auth, Quirks};

pub const MODEL_PREFIX: &str = "free:";

pub fn secret_reference(slug: &str) -> String {
    format!("free:{slug}")
}

const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(180);
const AUTH_COOLDOWN: Duration = Duration::from_secs(1800);
const GONE_COOLDOWN: Duration = Duration::from_secs(60);

pub const CATALOGUE_TTL: Duration = Duration::from_secs(1800);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FreeFlavour {
    Auto,
    Coding,
    Reasoning,
    Fast,
}

impl FreeFlavour {
    pub const ALL: &'static [FreeFlavour] =
        &[Self::Auto, Self::Coding, Self::Reasoning, Self::Fast];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Coding => "coding",
            Self::Reasoning => "reasoning",
            Self::Fast => "fast",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(Self::Auto),
            "coding" | "code" => Some(Self::Coding),
            "reasoning" | "think" | "thinking" => Some(Self::Reasoning),
            "fast" | "quick" | "small" => Some(Self::Fast),
            _ => None,
        }
    }

    pub fn model_id(self) -> String {
        format!("{MODEL_PREFIX}{}", self.as_str())
    }

    pub fn speciality(self) -> Option<Speciality> {
        match self {
            Self::Auto => None,
            Self::Coding => Some(Speciality::Coding),
            Self::Reasoning => Some(Speciality::Reasoning),
            Self::Fast => Some(Speciality::Fast),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Kuro Free",
            Self::Coding => "Kuro Free · coding",
            Self::Reasoning => "Kuro Free · reasoning",
            Self::Fast => "Kuro Free · fast",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Self::Auto => "The best free model among the keys you have added.",
            Self::Coding => "Prefers a code-trained model.",
            Self::Reasoning => "Prefers a model that reasons before answering.",
            Self::Fast => "Prefers a small model, for speed over depth.",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FreeModel {
    pub id: &'static str,
    pub flavours: &'static [FreeFlavour],
    pub rank: u8,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum FreeMarker {
    Everything,
    Suffix(&'static [&'static str]),
}

impl FreeMarker {
    pub fn covers(self, model_id: &str) -> bool {
        match self {
            Self::Everything => true,
            Self::Suffix(markers) => markers.iter().any(|marker| model_id.ends_with(marker)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum FreeTier {
    Recurring,
    Keyless,
    StarterCredit,
    ExpiringTrial { until: &'static str },
}

impl FreeTier {
    pub fn expired_on(self, today: &str) -> bool {
        matches!(self, Self::ExpiringTrial { until } if today > until)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recurring => "recurring",
            Self::Keyless => "keyless",
            Self::StarterCredit => "starter_credit",
            Self::ExpiringTrial { .. } => "expiring_trial",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Privacy {
    pub logged: bool,
    pub trains: bool,
}

impl Privacy {
    pub const STANDARD: Self = Self { logged: false, trains: false };
    pub const SHARED: Self = Self { logged: true, trains: true };
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FreeProvider {
    pub slug: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub credentials_url: &'static str,
    pub allowance: &'static str,
    pub key_hint: Option<&'static str>,
    pub free_marker: FreeMarker,
    pub tier: FreeTier,
    pub privacy: Privacy,
    pub quirks: Quirks,
    pub model_key_url: Option<&'static str>,
    pub models: &'static [FreeModel],
}

impl FreeProvider {
    pub fn is_reachable(&self, keys: &HashMap<String, String>, allow_keyless: bool) -> bool {
        if self.quirks.auth.keyless() {
            return allow_keyless || keys.contains_key(self.slug);
        }
        keys.contains_key(self.slug)
    }
}

pub const FREE_PROVIDERS: &[FreeProvider] = &[
    FreeProvider {
        slug: "groq",
        name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        credentials_url: "https://console.groq.com/keys",
        allowance: "A generous free tier with per-minute and per-day limits. Extremely fast.",
        key_hint: Some("gsk_…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[
            FreeModel {
                id: "llama-3.3-70b-versatile",
                flavours: &[FreeFlavour::Auto, FreeFlavour::Coding],
                rank: 0,
            },
            FreeModel {
                id: "llama-3.1-8b-instant",
                flavours: &[FreeFlavour::Fast],
                rank: 0,
            },
        ],
    },
    FreeProvider {
        slug: "cerebras",
        name: "Cerebras",
        base_url: "https://api.cerebras.ai/v1",
        credentials_url: "https://cloud.cerebras.ai/",
        allowance: "A free developer tier with a daily token allowance. The fastest of these.",
        key_hint: Some("csk-…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[
            FreeModel {
                id: "llama-3.3-70b",
                flavours: &[FreeFlavour::Auto, FreeFlavour::Coding],
                rank: 1,
            },
            FreeModel {
                id: "llama3.1-8b",
                flavours: &[FreeFlavour::Fast],
                rank: 1,
            },
        ],
    },
    FreeProvider {
        slug: "google",
        name: "Google AI Studio",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        credentials_url: "https://aistudio.google.com/apikey",
        allowance: "Gemini models with a free request allowance per day. No card required. \
                    The free tier reserves the right to use what it receives.",
        key_hint: Some("AIza…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::SHARED,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[
            FreeModel {
                id: "gemini-2.0-flash",
                flavours: &[FreeFlavour::Auto, FreeFlavour::Fast],
                rank: 0,
            },
            FreeModel {
                id: "gemini-2.0-flash-thinking-exp",
                flavours: &[FreeFlavour::Reasoning],
                rank: 0,
            },
        ],
    },
    FreeProvider {
        slug: "openrouter",
        name: "OpenRouter (free models)",
        base_url: "https://openrouter.ai/api/v1",
        credentials_url: "https://openrouter.ai/keys",
        allowance: "Model ids ending in :free cost nothing, with a shared daily request cap. \
                    Free models may be trained on.",
        key_hint: Some("sk-or-v1-…"),
        free_marker: FreeMarker::Suffix(&[":free"]),
        tier: FreeTier::Recurring,
        privacy: Privacy::SHARED,
        quirks: Quirks {
            headers: &[
                ("HTTP-Referer", "https://github.com/Krysis-ux/kuro"),
                ("X-Title", "Kuro"),
            ],
            ..Quirks::OPENAI
        },
        model_key_url: None,
        models: &[FreeModel {
            id: "openai/gpt-oss-20b:free",
            flavours: &[FreeFlavour::Auto, FreeFlavour::Coding],
            rank: 2,
        }],
    },
    FreeProvider {
        slug: "mistral",
        name: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        credentials_url: "https://console.mistral.ai/api-keys",
        allowance: "A free experimentation tier, rate limited per minute and per month.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[
            FreeModel {
                id: "mistral-small-latest",
                flavours: &[FreeFlavour::Auto, FreeFlavour::Fast],
                rank: 3,
            },
            FreeModel {
                id: "codestral-latest",
                flavours: &[FreeFlavour::Coding],
                rank: 3,
            },
        ],
    },
    FreeProvider {
        slug: "cohere",
        name: "Cohere",
        base_url: "https://api.cohere.ai/compatibility/v1",
        credentials_url: "https://dashboard.cohere.com/api-keys",
        allowance: "A free trial key with a monthly request allowance, rate limited per minute.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks {
            strip_schema_keywords: true,
            ..Quirks::OPENAI
        },
        model_key_url: None,
        models: &[
            FreeModel {
                id: "command-a-03-2025",
                flavours: &[FreeFlavour::Auto, FreeFlavour::Coding],
                rank: 4,
            },
            FreeModel {
                id: "command-r7b-12-2024",
                flavours: &[FreeFlavour::Fast],
                rank: 4,
            },
        ],
    },
    FreeProvider {
        slug: "cloudflare",
        name: "Cloudflare Workers AI",
        base_url: "https://api.cloudflare.com/client/v4/accounts/ACCOUNT_ID/ai/v1",
        credentials_url: "https://dash.cloudflare.com/profile/api-tokens",
        allowance: "A daily allowance of neurons on the free plan. The base URL needs your \
                    account id pasted into it.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[FreeModel {
            id: "@cf/meta/llama-3.1-8b-instruct",
            flavours: &[FreeFlavour::Fast],
            rank: 5,
        }],
    },
    FreeProvider {
        slug: "zhipu",
        name: "Z.ai (Zhipu)",
        base_url: "https://open.bigmodel.cn/api/paas/v4",
        credentials_url: "https://open.bigmodel.cn/usercenter/apikeys",
        allowance: "GLM models, with the flash variants free indefinitely. A mainland-China \
                    endpoint, so expect slower round trips than the rest of this list.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[FreeModel {
            id: "glm-4-flash",
            flavours: &[FreeFlavour::Auto, FreeFlavour::Fast],
            rank: 6,
        }],
    },
    FreeProvider {
        slug: "siliconflow",
        name: "SiliconFlow",
        base_url: "https://api.siliconflow.com/v1",
        credentials_url: "https://cloud.siliconflow.com/account/ak",
        allowance: "A set of smaller open-weight models that stay free, plus signup credit.",
        key_hint: Some("sk-…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[FreeModel {
            id: "Qwen/Qwen2.5-7B-Instruct",
            flavours: &[FreeFlavour::Auto, FreeFlavour::Fast],
            rank: 6,
        }],
    },
    FreeProvider {
        slug: "ollama",
        name: "Ollama Cloud",
        base_url: "https://ollama.com/v1",
        credentials_url: "https://ollama.com/settings/keys",
        allowance: "An hourly and daily allowance on the hosted models, for any account.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[
            FreeModel {
                id: "gpt-oss:20b",
                flavours: &[FreeFlavour::Auto],
                rank: 5,
            },
            FreeModel {
                id: "kimi-k2.7-code",
                flavours: &[FreeFlavour::Coding],
                rank: 5,
            },
        ],
    },
    FreeProvider {
        slug: "nvidia",
        name: "NVIDIA NIM",
        base_url: "https://integrate.api.nvidia.com/v1",
        credentials_url: "https://build.nvidia.com/",
        allowance: "Free API credits for developers, across a large catalogue. Note that one \
                    key does not reach every model: some are enabled per key. If a model here \
                    is greyed out, use the link on its row to generate a key from that model's \
                    own page, then paste it here.",
        key_hint: Some("nvapi-…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks {
            no_parallel_tool_calls: true,
            timeout: Some(Duration::from_secs(180)),
            ..Quirks::OPENAI
        },
        model_key_url: Some("https://build.nvidia.com/{model}"),
        models: &[
            FreeModel {
                id: "meta/llama-3.3-70b-instruct",
                flavours: &[FreeFlavour::Auto],
                rank: 7,
            },
            FreeModel {
                id: "qwen/qwen2.5-coder-32b-instruct",
                flavours: &[FreeFlavour::Coding],
                rank: 6,
            },
        ],
    },
    FreeProvider {
        slug: "vercel",
        name: "Vercel AI Gateway",
        base_url: "https://ai-gateway.vercel.sh/v1",
        credentials_url: "https://vercel.com/dashboard/ai-gateway/api-keys",
        allowance: "Free credit that renews every 30 days across a large catalogue — but only \
                    while the account has never bought gateway credit. Buying any ends it.",
        key_hint: Some("vck_…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[
            FreeModel {
                id: "alibaba/qwen-3-32b",
                flavours: &[FreeFlavour::Auto],
                rank: 7,
            },
            FreeModel {
                id: "alibaba/qwen3-coder",
                flavours: &[FreeFlavour::Coding],
                rank: 7,
            },
        ],
    },
    FreeProvider {
        slug: "speka",
        name: "Speka",
        base_url: "https://speka.me/v1",
        credentials_url: "https://speka.me",
        allowance: "A dollar of model usage every month, no card. How far that goes depends \
                    entirely on which model answers.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[
            FreeModel {
                id: "openai/gpt-oss-20b",
                flavours: &[FreeFlavour::Auto],
                rank: 8,
            },
            FreeModel {
                id: "meta/llama-3.1-8b-instruct",
                flavours: &[FreeFlavour::Fast],
                rank: 8,
            },
        ],
    },
    FreeProvider {
        slug: "scaleway",
        name: "Scaleway",
        base_url: "https://api.scaleway.ai/v1",
        credentials_url: "https://console.scaleway.com/iam/api-keys",
        allowance: "A free tier on their generative APIs, with a monthly token allowance.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[FreeModel {
            id: "llama-3.3-70b-instruct",
            flavours: &[FreeFlavour::Auto],
            rank: 9,
        }],
    },
    FreeProvider {
        slug: "huggingface",
        name: "Hugging Face Inference",
        base_url: "https://router.huggingface.co/v1",
        credentials_url: "https://huggingface.co/settings/tokens",
        allowance: "A monthly credit allowance on a free account, routed to several providers.",
        key_hint: Some("hf_…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[FreeModel {
            id: "meta-llama/Llama-3.3-70B-Instruct",
            flavours: &[FreeFlavour::Auto],
            rank: 10,
        }],
    },
    FreeProvider {
        slug: "llm7",
        name: "LLM7",
        base_url: "https://api.llm7.io/v1",
        credentials_url: "https://dash.llm7.io/#/api-keys",
        allowance: "A daily token allowance on a free key that needs no card. This answered \
                    anonymous requests until recently and no longer does.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks::OPENAI,
        model_key_url: None,
        models: &[FreeModel {
            id: "deepseek-v4-flash",
            flavours: &[FreeFlavour::Auto, FreeFlavour::Fast],
            rank: 11,
        }],
    },
    FreeProvider {
        slug: "freeai",
        name: "Free.ai",
        base_url: "https://api.free.ai/v1",
        credentials_url: "https://free.ai",
        allowance: "A daily token allowance on the models they host themselves. Models they \
                    resell from elsewhere are billed, and are not used here.",
        key_hint: Some("sk-free-…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks {
            chat_path: "/chat/",
            ..Quirks::OPENAI
        },
        model_key_url: None,
        models: &[
            FreeModel {
                id: "qwen7b",
                flavours: &[FreeFlavour::Auto],
                rank: 12,
            },
            FreeModel {
                id: "qwen-coder",
                flavours: &[FreeFlavour::Coding],
                rank: 12,
            },
        ],
    },
    FreeProvider {
        slug: "kilo",
        name: "Kilo Gateway (shared)",
        base_url: "https://api.kilo.ai/api/gateway/v1",
        credentials_url: "https://app.kilo.ai",
        allowance: "Answers with no account at all, around 200 requests an hour from one \
                    address. Free prompts may be logged and used to improve models.",
        key_hint: None,
        free_marker: FreeMarker::Suffix(&[":free", "kilo-auto/free"]),
        tier: FreeTier::Keyless,
        privacy: Privacy::SHARED,
        quirks: Quirks {
            auth: Auth::None,
            ..Quirks::OPENAI
        },
        model_key_url: None,
        models: &[FreeModel {
            id: "inclusionai/ling-3.0-flash:free",
            flavours: &[FreeFlavour::Auto, FreeFlavour::Fast],
            rank: 20,
        }],
    },
    FreeProvider {
        slug: "ovh",
        name: "OVHcloud AI Endpoints (shared)",
        base_url: "https://oai.endpoints.kepler.ai.cloud.ovh.net/v1",
        credentials_url: "https://endpoints.ai.cloud.ovh.net",
        allowance: "Answers with no account, but only about two requests a minute from one \
                    address, so expect to be turned away whenever it is busy.",
        key_hint: None,
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Keyless,
        privacy: Privacy::SHARED,
        quirks: Quirks {
            auth: Auth::None,
            ..Quirks::OPENAI
        },
        model_key_url: None,
        models: &[
            FreeModel {
                id: "Qwen3.5-9B",
                flavours: &[FreeFlavour::Auto, FreeFlavour::Fast],
                rank: 21,
            },
            FreeModel {
                id: "Qwen3.6-27B",
                flavours: &[FreeFlavour::Coding],
                rank: 21,
            },
        ],
    },
];

pub fn find(slug: &str) -> Option<&'static FreeProvider> {
    FREE_PROVIDERS
        .iter()
        .find(|provider| provider.slug.eq_ignore_ascii_case(slug))
}

fn model_rank(provider: &FreeProvider) -> u8 {
    provider
        .models
        .iter()
        .map(|model| model.rank)
        .min()
        .unwrap_or(u8::MAX)
}

pub fn is_free_model(model_id: &str) -> bool {
    parse_selection(model_id).is_some()
}

pub fn flavour_of(model_id: &str) -> Option<FreeFlavour> {
    match parse_selection(model_id)? {
        Selection::Flavour(flavour) => Some(flavour),
        Selection::Pinned { .. } => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Flavour(FreeFlavour),
    Pinned {
        slug: String,
        model: String,
    },
}

pub fn connector_id(slug: &str) -> String {
    format!("{MODEL_PREFIX}{slug}")
}

pub fn parse_selection(model_id: &str) -> Option<Selection> {
    let rest = model_id.strip_prefix(MODEL_PREFIX)?;

    if let Some(flavour) = FreeFlavour::parse(rest) {
        return Some(Selection::Flavour(flavour));
    }

    let (slug, model) = rest.split_once('/')?;
    if model.is_empty() {
        return None;
    }
    let provider = find(slug)?;

    Some(Selection::Pinned {
        slug: provider.slug.to_string(),
        model: model.to_string(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trouble {
    RateLimited,
    Rejected,
    Gone,
}

impl Trouble {
    pub fn cooldown(self) -> Duration {
        match self {
            Self::RateLimited => RATE_LIMIT_COOLDOWN,
            Self::Rejected => AUTH_COOLDOWN,
            Self::Gone => GONE_COOLDOWN,
        }
    }

    pub fn from_status(status: u16) -> Option<Self> {
        match status {
            401 | 403 => Some(Self::Rejected),
            402 | 429 => Some(Self::RateLimited),
            404 => Some(Self::Gone),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::Rejected => "rejected",
            Self::Gone => "model_gone",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "rate_limited" => Some(Self::RateLimited),
            "rejected" => Some(Self::Rejected),
            "model_gone" => Some(Self::Gone),
            _ => None,
        }
    }

    pub fn stale_catalogue(self) -> bool {
        self == Self::Gone
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub slug: String,
    pub name: String,
    pub base_url: String,
    pub authorization: Option<String>,
    pub model: String,
    pub quirks: Quirks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Quality {
    Matched,
    AnyCurated,
    Live,
}

#[derive(Clone, Default)]
pub struct FreePool {
    troubled: Arc<Mutex<HashMap<String, (Trouble, Instant)>>>,
    live: Arc<Mutex<Catalogues>>,
    allow_keyless: Arc<AtomicBool>,
    unavailable: Arc<Mutex<HashMap<String, (Trouble, Instant)>>>,
    refreshing: Arc<AtomicBool>,
}

type Catalogues = HashMap<String, (Vec<String>, Instant)>;

impl std::fmt::Debug for FreePool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("FreePool").finish()
    }
}

impl FreePool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn note_trouble(&self, slug: &str, trouble: Trouble) {
        self.troubled
            .lock()
            .expect("free pool lock")
            .insert(slug.to_string(), (trouble, Instant::now()));
        tracing::info!(slug, kind = trouble.as_str(), "free provider set aside");
    }

    pub fn clear_trouble(&self, slug: &str) {
        self.troubled.lock().expect("free pool lock").remove(slug);

        let prefix = format!("{slug}/");
        self.unavailable
            .lock()
            .expect("free pool lock")
            .retain(|key, _| !key.starts_with(&prefix));
    }

    pub fn note_model_trouble(&self, slug: &str, model: &str, trouble: Trouble) {
        self.unavailable
            .lock()
            .expect("free pool lock")
            .insert(format!("{slug}/{model}"), (trouble, Instant::now()));
        tracing::info!(slug, model, kind = trouble.as_str(), "free model set aside");
    }

    pub fn model_trouble(&self, slug: &str, model: &str) -> Option<Trouble> {
        let mut held = self.unavailable.lock().expect("free pool lock");
        let key = format!("{slug}/{model}");
        let (trouble, since) = *held.get(&key)?;
        if since.elapsed() >= trouble.cooldown() {
            held.remove(&key);
            return None;
        }
        Some(trouble)
    }

    pub fn set_allow_keyless(&self, allowed: bool) {
        self.allow_keyless.store(allowed, Ordering::Relaxed);
    }

    pub fn allows_keyless(&self) -> bool {
        self.allow_keyless.load(Ordering::Relaxed)
    }

    pub fn set_live_models(&self, slug: &str, models: Vec<String>) {
        self.live
            .lock()
            .expect("free pool lock")
            .insert(slug.to_string(), (models, Instant::now()));
    }

    pub fn troubles(&self) -> Vec<(String, &'static str, u64)> {
        let held = self.troubled.lock().expect("free pool lock");
        let models = self.unavailable.lock().expect("free pool lock");

        held.iter()
            .chain(models.iter())
            .filter_map(|(key, (trouble, since))| {
                let left = trouble.cooldown().checked_sub(since.elapsed())?;
                Some((key.clone(), trouble.as_str(), left.as_secs()))
            })
            .collect()
    }

    pub fn restore_troubles(&self, stored: Vec<(String, String, u64)>) {
        let now = Instant::now();

        for (key, kind, seconds_left) in stored {
            let Some(trouble) = Trouble::parse(&kind) else {
                continue;
            };
            let left = Duration::from_secs(seconds_left);
            let Some(elapsed) = trouble.cooldown().checked_sub(left) else {
                continue;
            };
            let Some(since) = now.checked_sub(elapsed) else {
                continue;
            };

            let target = if key.contains('/') {
                &self.unavailable
            } else {
                &self.troubled
            };
            target.lock().expect("free pool lock").insert(key, (trouble, since));
        }
    }

    pub fn catalogues(&self) -> HashMap<String, Vec<String>> {
        self.live
            .lock()
            .expect("free pool lock")
            .iter()
            .map(|(slug, (models, _))| (slug.clone(), models.clone()))
            .collect()
    }

    pub fn restore_catalogues(&self, stored: HashMap<String, Vec<String>>) {
        let mut held = self.live.lock().expect("free pool lock");
        let now = Instant::now();
        for (slug, models) in stored {
            held.entry(slug).or_insert((models, now));
        }
    }

    pub fn forget_live_models(&self, slug: &str) {
        self.live.lock().expect("free pool lock").remove(slug);
    }

    pub fn begin_refresh(&self) -> bool {
        self.refreshing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn end_refresh(&self) {
        self.refreshing.store(false, Ordering::Release);
    }

    pub fn needs_any_catalogue(&self, keys: &HashMap<String, String>) -> bool {
        let allow_keyless = self.allows_keyless();
        FREE_PROVIDERS
            .iter()
            .filter(|provider| provider.is_reachable(keys, allow_keyless))
            .any(|provider| self.needs_catalogue(provider.slug))
    }

    pub fn needs_catalogue(&self, slug: &str) -> bool {
        self.live
            .lock()
            .expect("free pool lock")
            .get(slug)
            .is_none_or(|(_, read_at)| read_at.elapsed() >= CATALOGUE_TTL)
    }

    fn advertises(&self, slug: &str, model: &str) -> bool {
        let held = self.live.lock().expect("free pool lock");
        let Some((models, _)) = held.get(slug) else {
            return true;
        };
        models.is_empty() || models.iter().any(|known| known == model)
    }

    pub fn trouble(&self, slug: &str) -> Option<Trouble> {
        let mut held = self.troubled.lock().expect("free pool lock");
        let (trouble, since) = *held.get(slug)?;
        if since.elapsed() >= trouble.cooldown() {
            held.remove(slug);
            return None;
        }
        Some(trouble)
    }

    pub fn choose(&self, flavour: FreeFlavour, keys: &HashMap<String, String>) -> Option<Choice> {
        let allow_keyless = self.allows_keyless();
        let usable = |provider: &FreeProvider| {
            provider.is_reachable(keys, allow_keyless) && self.trouble(provider.slug).is_none()
        };

        let mut candidates: Vec<(bool, Quality, u8, &FreeProvider, String)> = Vec::new();

        for provider in FREE_PROVIDERS.iter().filter(|held| usable(held)) {
            let keyless = provider.quirks.auth.keyless() && !keys.contains_key(provider.slug);

            for model in provider.models {
                if !self.advertises(provider.slug, model.id) {
                    continue;
                }
                if self.model_trouble(provider.slug, model.id).is_some() {
                    continue;
                }
                if model.flavours.contains(&flavour) {
                    candidates.push((
                        keyless,
                        Quality::Matched,
                        model.rank,
                        provider,
                        model.id.to_string(),
                    ));
                } else if flavour != FreeFlavour::Auto {
                    candidates.push((
                        keyless,
                        Quality::AnyCurated,
                        model.rank,
                        provider,
                        model.id.to_string(),
                    ));
                }
            }

            if let Some(model) = self.best_live_model(provider, flavour) {
                candidates.push((keyless, Quality::Live, model_rank(provider), provider, model));
            }
        }

        candidates.sort_by_key(|(keyless, quality, rank, _, _)| (*keyless, *quality, *rank));

        let (_, _, _, provider, model) = candidates.first()?;

        Some(Choice {
            slug: provider.slug.to_string(),
            name: provider.name.to_string(),
            base_url: provider.base_url.to_string(),
            authorization: provider
                .quirks
                .auth
                .authorization(keys.get(provider.slug).map(String::as_str)),
            model: model.clone(),
            quirks: provider.quirks,
        })
    }

    fn best_live_model(&self, provider: &FreeProvider, flavour: FreeFlavour) -> Option<String> {
        let held = self.live.lock().expect("free pool lock");
        let (models, _) = held.get(provider.slug)?;

        let wanted = flavour.speciality();

        let mut usable: Vec<(bool, &String)> = models
            .iter()
            .filter(|model| provider.free_marker.covers(model))
            .filter_map(|model| {
                let classified = classify(model);
                if !classified.kind.is_chat() {
                    return None;
                }
                let matches = match wanted {
                    Some(speciality) => classified.has(speciality),
                    None => !classified.has(Speciality::Fast),
                };
                Some((!matches, model))
            })
            .collect();

        usable.sort();
        usable.first().map(|(_, model)| (*model).clone())
    }

    pub fn pinned(
        &self,
        slug: &str,
        model: &str,
        keys: &HashMap<String, String>,
    ) -> Option<Choice> {
        let provider = find(slug)?;
        if !provider.is_reachable(keys, self.allows_keyless()) {
            return None;
        }
        if self.trouble(provider.slug).is_some() {
            return None;
        }

        Some(Choice {
            slug: provider.slug.to_string(),
            name: provider.name.to_string(),
            base_url: provider.base_url.to_string(),
            authorization: provider
                .quirks
                .auth
                .authorization(keys.get(provider.slug).map(String::as_str)),
            model: model.to_string(),
            quirks: provider.quirks,
        })
    }

    pub fn advertised_chat_models(&self, provider: &FreeProvider) -> Vec<String> {
        let held = self.live.lock().expect("free pool lock");
        let Some((models, _)) = held.get(provider.slug) else {
            return Vec::new();
        };

        let mut usable: Vec<String> = models
            .iter()
            .filter(|model| provider.free_marker.covers(model))
            .filter(|model| classify(model).kind.is_chat())
            .cloned()
            .collect();
        usable.sort();
        usable.dedup();
        usable
    }

    pub fn available(&self, keys: &HashMap<String, String>) -> Vec<&'static FreeProvider> {
        FREE_PROVIDERS
            .iter()
            .filter(|provider| {
                keys.contains_key(provider.slug) && self.trouble(provider.slug).is_none()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(slugs: &[&str]) -> HashMap<String, String> {
        slugs
            .iter()
            .map(|slug| ((*slug).to_string(), format!("key-for-{slug}")))
            .collect()
    }

    #[test]
    fn every_slug_is_unique_and_findable() {
        let mut seen: Vec<&str> = Vec::new();
        for provider in FREE_PROVIDERS {
            assert!(!seen.contains(&provider.slug), "duplicate `{}`", provider.slug);
            seen.push(provider.slug);
            assert!(find(provider.slug).is_some());
        }
        assert!(find("nothing-like-this").is_none());
    }

    #[test]
    fn every_provider_says_where_its_key_comes_from_and_what_the_limit_is() {
        for provider in FREE_PROVIDERS {
            assert!(provider.credentials_url.starts_with("https://"), "{}", provider.slug);
            assert!(provider.allowance.len() > 25, "{}", provider.slug);
            assert!(provider.base_url.starts_with("https://"), "{}", provider.slug);
            assert!(!provider.models.is_empty(), "{}", provider.slug);
        }
    }

    #[test]
    fn base_urls_are_bases_rather_than_endpoints() {
        for provider in FREE_PROVIDERS {
            assert!(
                !provider.base_url.contains("chat/completions"),
                "`{}` should carry a base URL",
                provider.slug
            );
        }
    }

    #[test]
    fn every_flavour_can_be_served_by_something() {
        let everything: HashMap<String, String> = FREE_PROVIDERS
            .iter()
            .map(|provider| (provider.slug.to_string(), "k".to_string()))
            .collect();
        let pool = FreePool::new();

        for flavour in FreeFlavour::ALL {
            assert!(
                pool.choose(*flavour, &everything).is_some(),
                "`{}` would be a model that never resolves",
                flavour.as_str()
            );
        }
    }

    #[test]
    fn nothing_resolves_until_the_user_has_added_a_key() {
        let pool = FreePool::new();
        assert!(pool.choose(FreeFlavour::Auto, &HashMap::new()).is_none());
        assert!(pool.available(&HashMap::new()).is_empty());
    }

    #[test]
    fn the_chosen_provider_is_one_the_user_holds_a_key_for() {
        let pool = FreePool::new();
        let chosen = pool
            .choose(FreeFlavour::Auto, &keys(&["mistral"]))
            .expect("a provider");

        assert_eq!(chosen.slug, "mistral");
        assert_eq!(chosen.authorization.as_deref(), Some("Bearer key-for-mistral"));
        assert!(!chosen.model.is_empty());
    }

    #[test]
    fn a_specific_flavour_prefers_a_model_that_matches_it() {
        let pool = FreePool::new();

        let coding = pool
            .choose(FreeFlavour::Coding, &keys(&["ollama"]))
            .expect("a provider");
        assert_eq!(coding.model, "kimi-k2.7-code", "got {}", coding.model);

        let auto = pool.choose(FreeFlavour::Auto, &keys(&["ollama"])).expect("a provider");
        assert_eq!(auto.model, "gpt-oss:20b", "got {}", auto.model);

        let fast = pool.choose(FreeFlavour::Fast, &keys(&["groq"])).expect("a provider");
        assert_eq!(fast.model, "llama-3.1-8b-instant", "got {}", fast.model);
    }

    #[test]
    fn one_key_answers_every_flavour_rather_than_failing_on_a_specific_one() {
        let pool = FreePool::new();
        let chosen = pool.choose(FreeFlavour::Reasoning, &keys(&["cloudflare"]));

        assert!(chosen.is_some(), "a specific flavour must not strand a working key");
    }

    #[test]
    fn a_provider_that_refused_is_skipped_and_the_next_one_answers() {
        let pool = FreePool::new();
        let held = keys(&["groq", "mistral"]);

        assert_eq!(pool.choose(FreeFlavour::Auto, &held).unwrap().slug, "groq");

        pool.note_trouble("groq", Trouble::RateLimited);

        let after = pool.choose(FreeFlavour::Auto, &held).expect("failover");
        assert_eq!(after.slug, "mistral", "one exhausted allowance is not an outage");
        assert_eq!(pool.trouble("groq"), Some(Trouble::RateLimited));
    }

    #[test]
    fn replacing_a_key_clears_the_trouble_it_was_in() {
        let pool = FreePool::new();
        pool.note_trouble("groq", Trouble::Rejected);
        assert!(pool.trouble("groq").is_some());

        pool.clear_trouble("groq");

        assert!(pool.trouble("groq").is_none());
        assert_eq!(pool.choose(FreeFlavour::Auto, &keys(&["groq"])).unwrap().slug, "groq");
    }

    #[test]
    fn a_curated_model_the_provider_no_longer_lists_is_not_asked_for() {
        let pool = FreePool::new();
        let held = keys(&["openrouter"]);

        pool.set_live_models(
            "openrouter",
            vec![
                "openai/gpt-oss-20b:free".to_string(),
                "google/gemma-4-31b-it:free".to_string(),
            ],
        );

        let chosen = pool.choose(FreeFlavour::Auto, &held).expect("a model");

        assert_eq!(chosen.slug, "openrouter");
        assert!(
            ["openai/gpt-oss-20b:free", "google/gemma-4-31b-it:free"].contains(&chosen.model.as_str()),
            "asked for `{}`, which the provider does not offer",
            chosen.model
        );
    }

    #[test]
    fn a_curated_model_that_is_still_listed_keeps_its_flavour() {
        let pool = FreePool::new();
        pool.set_live_models(
            "ollama",
            vec!["gpt-oss:20b".to_string(), "kimi-k2.7-code".to_string()],
        );

        let coding = pool.choose(FreeFlavour::Coding, &keys(&["ollama"])).expect("a model");

        assert_eq!(coding.model, "kimi-k2.7-code", "got {}", coding.model);
    }

    #[test]
    fn a_provider_kuro_has_not_asked_yet_is_still_tried() {
        let pool = FreePool::new();
        assert!(pool.needs_catalogue("groq"));
        assert!(pool.choose(FreeFlavour::Auto, &keys(&["groq"])).is_some());
    }

    #[test]
    fn an_empty_catalogue_is_treated_as_unknown_rather_than_as_nothing_offered() {
        let pool = FreePool::new();
        pool.set_live_models("groq", Vec::new());

        assert!(pool.choose(FreeFlavour::Auto, &keys(&["groq"])).is_some());
    }

    #[test]
    fn when_every_curated_model_is_gone_the_live_list_is_used_instead() {
        let pool = FreePool::new();
        pool.set_live_models("openrouter", vec!["some/model-nobody-curated:free".to_string()]);

        let chosen = pool
            .choose(FreeFlavour::Auto, &keys(&["openrouter"]))
            .expect("a working key must not be refused");

        assert_eq!(chosen.model, "some/model-nobody-curated:free");
    }

    #[test]
    fn the_live_fallback_never_reaches_for_a_model_that_would_be_billed() {
        let pool = FreePool::new();
        pool.set_live_models(
            "openrouter",
            vec![
                "ai21/jamba-large-1.7".to_string(),
                "vendor/model-name".to_string(),
                "openai/gpt-oss-20b:free".to_string(),
                "zzz/last-alphabetically".to_string(),
            ],
        );

        let chosen = pool
            .choose(FreeFlavour::Auto, &keys(&["openrouter"]))
            .expect("a free model is available");

        assert_eq!(chosen.model, "openai/gpt-oss-20b:free");
    }

    #[test]
    fn a_wallet_provider_with_nothing_free_left_is_skipped_rather_than_billed() {
        let pool = FreePool::new();
        pool.set_live_models("openrouter", vec!["vendor/model-name".to_string()]);

        assert!(
            pool.choose(FreeFlavour::Auto, &keys(&["openrouter"])).is_none(),
            "no free model is not the same as any model"
        );
    }

    #[test]
    fn an_account_level_free_tier_may_use_anything_it_lists() {
        let pool = FreePool::new();
        pool.set_live_models("groq", vec!["some-new-groq-model".to_string()]);

        let chosen = pool.choose(FreeFlavour::Auto, &keys(&["groq"])).expect("a model");
        assert_eq!(chosen.model, "some-new-groq-model");
    }

    fn nvidia_catalogue() -> Vec<String> {
        [
            "baai/bge-m3",
            "black-forest-labs/flux.1-dev",
            "meta/llama-3.3-70b-instruct",
            "meta/llama-guard-4-12b",
            "nvidia/llama-3.2-nv-embedqa-1b-v2",
            "nvidia/llama-3.2-nv-rerankqa-1b-v2",
            "nvidia/parakeet-ctc-1.1b-asr",
            "qwen/qwen2.5-coder-32b-instruct",
        ]
        .iter()
        .map(|id| (*id).to_string())
        .collect()
    }

    #[test]
    fn a_catalogue_full_of_things_that_are_not_chat_models_still_yields_a_chat_model() {
        let pool = FreePool::new();
        pool.set_live_models("nvidia", nvidia_catalogue());

        let live_only: Vec<String> = nvidia_catalogue()
            .into_iter()
            .filter(|id| id != "meta/llama-3.3-70b-instruct" && !id.contains("coder"))
            .collect();
        pool.set_live_models("nvidia", live_only);

        assert!(
            pool.choose(FreeFlavour::Auto, &keys(&["nvidia"])).is_none(),
            "nothing in that list can hold a conversation, and inventing one \
             would mean every message failing"
        );
    }

    #[test]
    fn the_live_fallback_keeps_the_flavour_it_was_asked_for() {
        let pool = FreePool::new();
        pool.set_live_models("nvidia", nvidia_catalogue());

        let coding = pool
            .choose(FreeFlavour::Coding, &keys(&["nvidia"]))
            .expect("a provider");
        assert_eq!(coding.model, "qwen/qwen2.5-coder-32b-instruct");

        let auto = pool
            .choose(FreeFlavour::Auto, &keys(&["nvidia"]))
            .expect("a provider");
        assert_eq!(auto.model, "meta/llama-3.3-70b-instruct");
    }

    #[test]
    fn only_one_catalogue_read_runs_at_a_time() {
        let pool = FreePool::new();

        assert!(pool.begin_refresh(), "the first caller does the work");
        assert!(!pool.begin_refresh(), "the second finds one already running");

        pool.end_refresh();
        assert!(pool.begin_refresh(), "and the next one after it finishes may go");
    }

    #[test]
    fn a_fresh_catalogue_does_not_start_a_read_at_all() {
        let pool = FreePool::new();
        let held = keys(&["groq"]);

        assert!(
            pool.needs_any_catalogue(&held),
            "nothing has been read yet, so there is work to do"
        );

        pool.set_live_models("groq", vec!["llama-3.3-70b-versatile".to_string()]);
        assert!(
            !pool.needs_any_catalogue(&held),
            "the only reachable provider was just read"
        );
    }

    #[test]
    fn a_model_id_naming_one_provider_and_model_survives_the_round_trip() {
        let parsed = parse_selection("free:nvidia/meta/llama-3.3-70b-instruct");

        assert_eq!(
            parsed,
            Some(Selection::Pinned {
                slug: "nvidia".to_string(),
                model: "meta/llama-3.3-70b-instruct".to_string(),
            })
        );
    }

    #[test]
    fn a_flavour_is_still_read_as_a_flavour_and_not_as_a_provider() {
        assert_eq!(
            parse_selection("free:coding"),
            Some(Selection::Flavour(FreeFlavour::Coding))
        );
        assert_eq!(flavour_of("free:coding"), Some(FreeFlavour::Coding));
        assert_eq!(flavour_of("free:nvidia/meta/llama-3.3-70b-instruct"), None);
    }

    #[test]
    fn an_id_naming_a_provider_kuro_does_not_have_is_not_a_free_model() {
        assert_eq!(parse_selection("free:notaprovider/some-model"), None);
        assert!(!is_free_model("free:notaprovider/some-model"));
        assert!(!is_free_model("free:nvidia/"));
        assert!(!is_free_model("free:nvidia"));
    }

    #[test]
    fn one_model_refusing_does_not_take_the_provider_down_with_it() {
        let pool = FreePool::new();
        let held = keys(&["nvidia"]);

        pool.note_model_trouble("nvidia", "deepseek-ai/deepseek-v4-pro", Trouble::Gone);

        assert!(pool.model_trouble("nvidia", "deepseek-ai/deepseek-v4-pro").is_some());
        assert!(
            pool.model_trouble("nvidia", "meta/llama-3.3-70b-instruct").is_none(),
            "a sibling model was set aside too"
        );
        assert!(pool.trouble("nvidia").is_none(), "the provider was set aside");
        assert!(
            pool.pinned("nvidia", "meta/llama-3.3-70b-instruct", &held).is_some(),
            "the key still works for everything else"
        );
    }

    #[test]
    fn a_refusal_survives_a_restart() {
        let before = FreePool::new();
        before.note_trouble("cerebras", Trouble::RateLimited);
        before.note_model_trouble("nvidia", "some/model", Trouble::Gone);

        let stored: Vec<(String, String, u64)> = before
            .troubles()
            .into_iter()
            .map(|(key, kind, left)| (key, kind.to_string(), left))
            .collect();
        assert_eq!(stored.len(), 2);

        let after = FreePool::new();
        after.restore_troubles(stored);

        assert_eq!(after.trouble("cerebras"), Some(Trouble::RateLimited));
        assert_eq!(after.model_trouble("nvidia", "some/model"), Some(Trouble::Gone));
        assert!(after.trouble("nvidia").is_none());
    }

    #[test]
    fn a_refusal_that_has_already_expired_is_not_restored() {
        let pool = FreePool::new();
        pool.restore_troubles(vec![("groq".to_string(), "rate_limited".to_string(), 0)]);

        assert!(pool.trouble("groq").is_none());
    }

    #[test]
    fn replacing_a_key_clears_the_models_it_had_refused() {
        let pool = FreePool::new();
        pool.note_model_trouble("nvidia", "some/model", Trouble::Gone);
        pool.note_trouble("nvidia", Trouble::Rejected);

        pool.clear_trouble("nvidia");

        assert!(pool.trouble("nvidia").is_none());
        assert!(pool.model_trouble("nvidia", "some/model").is_none());
    }

    #[test]
    fn a_pinned_choice_does_not_fail_over_to_another_company() {
        let pool = FreePool::new();
        let chosen = pool
            .pinned("nvidia", "meta/llama-3.3-70b-instruct", &keys(&["nvidia", "groq"]))
            .expect("a key for nvidia");

        assert_eq!(chosen.slug, "nvidia");
        assert_eq!(chosen.model, "meta/llama-3.3-70b-instruct");

        assert!(pool
            .pinned("nvidia", "meta/llama-3.3-70b-instruct", &keys(&["groq"]))
            .is_none());
    }

    #[test]
    fn only_chat_models_are_offered_as_a_providers_catalogue() {
        let pool = FreePool::new();
        pool.set_live_models("nvidia", nvidia_catalogue());

        let listed = pool.advertised_chat_models(find("nvidia").expect("nvidia"));

        assert_eq!(
            listed,
            vec![
                "meta/llama-3.3-70b-instruct".to_string(),
                "qwen/qwen2.5-coder-32b-instruct".to_string(),
            ],
            "the embedding, rerank, guard, speech and image entries are not \
             models anybody can talk to"
        );
    }

    #[test]
    fn a_catalogue_nobody_has_read_yet_is_listed_as_nothing_rather_than_guessed_at() {
        let pool = FreePool::new();
        assert!(pool
            .advertised_chat_models(find("nvidia").expect("nvidia"))
            .is_empty());
    }

    #[test]
    fn a_guard_model_is_never_offered_as_a_conversation() {
        let pool = FreePool::new();
        pool.set_live_models(
            "nvidia",
            vec![
                "meta/llama-guard-4-12b".to_string(),
                "meta/llama-3.3-70b-instruct".to_string(),
            ],
        );

        let chosen = pool
            .choose(FreeFlavour::Auto, &keys(&["nvidia"]))
            .expect("a provider");
        assert_eq!(chosen.model, "meta/llama-3.3-70b-instruct");
    }

    #[test]
    fn every_curated_model_is_one_its_provider_marks_as_free() {
        for provider in FREE_PROVIDERS {
            for model in provider.models {
                assert!(
                    provider.free_marker.covers(model.id),
                    "`{}` lists `{}`, which its own free marker excludes",
                    provider.slug,
                    model.id
                );
            }
        }
    }

    #[test]
    fn the_wallet_providers_are_marked_by_suffix_and_the_rest_by_account() {
        let by_suffix: Vec<&str> = FREE_PROVIDERS
            .iter()
            .filter(|provider| matches!(provider.free_marker, FreeMarker::Suffix(_)))
            .map(|provider| provider.slug)
            .collect();

        assert_eq!(by_suffix, vec!["openrouter", "kilo"]);

        assert!(FreeMarker::Suffix(&[":free"]).covers("a/b:free"));
        assert!(!FreeMarker::Suffix(&[":free"]).covers("a/b"));
        assert!(FreeMarker::Everything.covers("anything-at-all"));
    }

    #[test]
    fn a_marker_can_carry_more_than_one_convention() {
        let kilo = find("kilo").expect("kilo");

        assert!(kilo.free_marker.covers("inclusionai/ling-3.0-flash:free"));
        assert!(kilo.free_marker.covers("kilo-auto/free"));
        assert!(!kilo.free_marker.covers("vendor/model-name"));
    }

    #[test]
    fn the_retired_providers_are_gone_and_do_not_resolve() {
        for slug in ["github", "together", "chutes"] {
            assert!(find(slug).is_none(), "`{slug}` is still in the table");
        }
    }

    #[test]
    fn every_keyless_provider_says_it_is_shared() {
        for provider in FREE_PROVIDERS {
            let keyless = provider.quirks.auth.keyless();
            assert_eq!(
                keyless,
                provider.tier == FreeTier::Keyless,
                "`{}` disagrees with itself about being keyless",
                provider.slug
            );
            if keyless {
                assert_eq!(
                    provider.privacy,
                    Privacy::SHARED,
                    "`{}` is a shared endpoint and must say so",
                    provider.slug
                );
            }
        }
    }

    #[test]
    fn an_expiring_trial_lapses_on_its_own_date() {
        let trial = FreeTier::ExpiringTrial { until: "2026-09-30" };

        assert!(!trial.expired_on("2026-09-30"), "the last day is still free");
        assert!(trial.expired_on("2026-10-01"));
        assert!(!FreeTier::Recurring.expired_on("2099-01-01"), "recurring never lapses");
        assert!(!FreeTier::Keyless.expired_on("2099-01-01"));
    }

    #[test]
    fn shared_endpoints_stay_silent_until_they_are_allowed() {
        let pool = FreePool::new();
        assert!(!pool.allows_keyless());
        assert!(
            pool.choose(FreeFlavour::Auto, &HashMap::new()).is_none(),
            "nothing should answer before the user has said it may"
        );

        pool.set_allow_keyless(true);

        let chosen = pool
            .choose(FreeFlavour::Auto, &HashMap::new())
            .expect("a shared endpoint answers once allowed");
        assert!(matches!(find(&chosen.slug).map(|p| p.tier), Some(FreeTier::Keyless)));
        assert_eq!(chosen.authorization, None, "a keyless endpoint gets no header");
    }

    #[test]
    fn a_key_beats_a_shared_endpoint_even_on_its_worst_fallback() {
        let pool = FreePool::new();
        pool.set_allow_keyless(true);

        pool.set_live_models("cerebras", vec!["some-new-cerebras-model".to_string()]);

        let chosen = pool
            .choose(FreeFlavour::Coding, &keys(&["cerebras"]))
            .expect("a choice");

        assert_eq!(chosen.slug, "cerebras", "a shared endpoint must not outrank a key");
    }

    #[test]
    fn shared_endpoints_answer_only_when_nothing_else_can() {
        let pool = FreePool::new();
        pool.set_allow_keyless(true);

        pool.note_trouble("groq", Trouble::RateLimited);

        let chosen = pool.choose(FreeFlavour::Auto, &keys(&["groq"])).expect("a choice");

        assert!(
            matches!(find(&chosen.slug).map(|p| p.tier), Some(FreeTier::Keyless)),
            "got `{}`, expected a shared endpoint to cover the gap",
            chosen.slug
        );
    }

    #[test]
    fn a_key_stored_for_a_shared_endpoint_is_used_and_promotes_it() {
        let pool = FreePool::new();
        let chosen = pool
            .choose(FreeFlavour::Auto, &keys(&["kilo"]))
            .expect("a stored key is reason enough");

        assert_eq!(chosen.slug, "kilo");
    }

    #[test]
    fn a_stale_catalogue_is_re_read_and_a_fresh_one_is_not() {
        let pool = FreePool::new();
        assert!(pool.needs_catalogue("groq"), "never read");

        pool.set_live_models("groq", vec!["llama-3.3-70b-versatile".to_string()]);
        assert!(!pool.needs_catalogue("groq"), "just read");

        pool.forget_live_models("groq");
        assert!(pool.needs_catalogue("groq"), "forgotten after a 404");
    }

    #[test]
    fn a_missing_model_is_recorded_as_such_and_clears_quickly() {
        assert_eq!(Trouble::from_status(404), Some(Trouble::Gone));
        assert!(Trouble::Gone.stale_catalogue());
        assert!(!Trouble::RateLimited.stale_catalogue());
        assert!(
            Trouble::Gone.cooldown() < Trouble::RateLimited.cooldown(),
            "a catalogue is fixed by re-reading it, not by waiting"
        );
    }

    #[test]
    fn a_rejected_key_is_set_aside_for_longer_than_a_busy_one() {
        assert!(Trouble::Rejected.cooldown() > Trouble::RateLimited.cooldown());

        assert_eq!(Trouble::from_status(429), Some(Trouble::RateLimited));
        assert_eq!(Trouble::from_status(401), Some(Trouble::Rejected));
        assert_eq!(Trouble::from_status(500), None, "a server error is not the key's fault");
        assert_eq!(Trouble::from_status(200), None);
    }

    #[test]
    fn free_model_ids_are_recognised_and_nothing_else_is() {
        assert!(is_free_model("free:auto"));
        assert!(is_free_model("free:coding"));
        assert_eq!(flavour_of("free:reasoning"), Some(FreeFlavour::Reasoning));

        assert!(!is_free_model("free:nonsense"));
        assert!(!is_free_model("cloud:abc/gpt-4o"));
        assert!(!is_free_model("qwen3-4b:q4_k_m"));
        assert_eq!(flavour_of("qwen3-4b:q4_k_m"), None);
    }

    #[test]
    fn every_flavour_round_trips_through_its_model_id() {
        for flavour in FreeFlavour::ALL {
            let id = flavour.model_id();
            assert!(is_free_model(&id));
            assert_eq!(flavour_of(&id), Some(*flavour));
            assert!(!flavour.label().is_empty());
            assert!(!flavour.blurb().is_empty());
        }
    }
}
