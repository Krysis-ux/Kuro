//! One model id, many free tiers behind it.
//!
//! Every company running an inference API gives some of it away. Groq, Cerebras,
//! Google, Mistral, GitHub, Cloudflare and a dozen others each hand out a daily
//! or monthly allowance that costs nothing and needs no card. Individually each
//! one is a toy. Together they are a real budget — and the reason nobody uses
//! them that way is that it means holding a dozen keys and remembering which one
//! still has quota left this hour.
//!
//! So that bookkeeping moves here. The user pastes in whichever free keys they
//! have, and gets back a single model called Kuro Free. Picking it sends the
//! request to whichever provider is holding a key, is not currently rate-limited,
//! and offers a model suited to what was asked for.
//!
//! ## What this is not
//!
//! It is not a key that Kuro supplies. There is no shared account, no proxy Kuro
//! runs, and nothing here works until the user has pasted in at least one key of
//! their own — every entry in [`FREE_PROVIDERS`] links to the page where its key
//! comes from. That is a real limitation and the screen says so, because the
//! alternative reading — that Kuro is giving away inference — is one somebody
//! would otherwise reasonably arrive at.
//!
//! It is also not free of consequence. These requests leave the machine, exactly
//! as a paid provider's would, and free tiers are the ones most likely to train
//! on what they receive. The interface says that too.
//!
//! ## Failover
//!
//! A provider that refuses is remembered for a cooldown rather than retried, so
//! one exhausted allowance does not slow every subsequent message down while it
//! is rediscovered. A 429 costs a few minutes; an auth failure costs longer,
//! because a rejected key is usually a wrong key rather than a busy one.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::classify::{classify, Speciality};
use crate::wire::{Auth, Quirks};

/// Prefix marking a model as belonging to the free pool rather than to one
/// provider. Deliberately distinct from `cloud:`, because the routing is
/// different: `cloud:` names one endpoint, this one names a preference order.
pub const MODEL_PREFIX: &str = "free:";

/// Credential-store key for one provider's free key.
pub fn secret_reference(slug: &str) -> String {
    format!("free:{slug}")
}

/// How long a provider is skipped after refusing.
const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(180);
/// Longer, because a rejected key does not usually start working on its own.
const AUTH_COOLDOWN: Duration = Duration::from_secs(1800);
/// Short: a missing model is fixed by re-reading the catalogue, not by waiting.
const GONE_COOLDOWN: Duration = Duration::from_secs(60);

/// How long a provider's live model list is trusted before being read again.
pub const CATALOGUE_TTL: Duration = Duration::from_secs(1800);

/// What a request is asking for, so the pool can pick a suitable model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FreeFlavour {
    /// Whatever is best available. The default and the one most people want.
    Auto,
    /// Prefer a model trained on code.
    Coding,
    /// Prefer a model that thinks before answering.
    Reasoning,
    /// Prefer something small and quick.
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

    /// The model id the picker shows.
    pub fn model_id(self) -> String {
        format!("{MODEL_PREFIX}{}", self.as_str())
    }

    /// What this flavour is looking for in a model it has never seen.
    ///
    /// `Auto` maps to nothing on purpose. It is not asking for a speciality —
    /// it is asking for the best general answer, and matching it against
    /// [`Speciality::General`] would rule out a strong coder for no reason.
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

/// One model a provider offers for nothing.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FreeModel {
    /// The name the provider knows it by, sent verbatim in the request body.
    pub id: &'static str,
    /// Which flavours this model is a good answer to.
    pub flavours: &'static [FreeFlavour],
    /// Rough ordering within a flavour. Lower is preferred.
    pub rank: u8,
}

/// How to tell which of a provider's advertised models cost nothing.
///
/// This exists because the two kinds of free tier are structurally different,
/// and confusing them costs the user money. On Groq or Cerebras the *account* is
/// free: it has no card attached, so anything it can call is within the
/// allowance. On OpenRouter the account is a wallet that may well have money in
/// it, and only ids carrying a marker are free — the other three hundred are
/// billed. A pool that reads "every model this key can reach" as "every model
/// this key can reach for nothing" would quietly spend real money on a screen
/// whose entire promise is that it does not.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum FreeMarker {
    /// Everything this key can reach is inside the free allowance.
    Everything,
    /// Only ids ending in one of these markers are free; the rest are billed.
    ///
    /// A slice rather than a single suffix because a gateway can have more than
    /// one convention — Kilo marks both `…:free` and its automatic router
    /// `kilo-auto/free` — and a second enum variant would be the same idea
    /// written twice.
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

/// What kind of free this is.
///
/// Not decorative. A recurring allowance is something to route to by default; a
/// starter credit is something that will one day stop working and start costing
/// money, which is the outcome this whole screen exists to rule out. Keeping
/// them in one list without saying which is which is how somebody ends up
/// billed by a screen labelled "free".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum FreeTier {
    /// Renews on its own. A daily or monthly allowance that comes back.
    Recurring,
    /// No account at all: shared, rate-limited by address, least private.
    Keyless,
    /// A one-off grant. Runs out and does not return.
    StarterCredit,
    /// Free until a date, billable after it. `until` is an ISO date.
    ExpiringTrial { until: &'static str },
}

impl FreeTier {
    /// Whether this tier has stopped being free.
    ///
    /// ISO dates compare lexicographically, so this needs no date parsing and
    /// no clock of its own — the caller passes today in, which is also what
    /// makes it testable without freezing time.
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

/// What a provider may do with what it is sent.
///
/// Free tiers are the ones most likely to read and train on what they receive,
/// and a shared anonymous endpoint is the most likely of all. Saying so per
/// provider — rather than once, vaguely, at the top of the screen — is what
/// lets somebody keep one conversation off the endpoints that log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Privacy {
    /// Prompts are retained somewhere the provider's staff can read them.
    pub logged: bool,
    /// Prompts may be used to train models.
    pub trains: bool,
}

impl Privacy {
    /// A provider that makes no unusual claim over what it is sent.
    pub const STANDARD: Self = Self { logged: false, trains: false };
    /// A shared endpoint, or a tier whose terms reserve the right to train.
    pub const SHARED: Self = Self { logged: true, trains: true };
}

/// A provider with an allowance worth having.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FreeProvider {
    pub slug: &'static str,
    pub name: &'static str,
    /// OpenAI-compatible base URL. Everything here speaks that shape, which is
    /// what makes one code path enough.
    pub base_url: &'static str,
    /// Where the key comes from. Every entry has one, because a provider whose
    /// key nobody can find is not a usable option.
    pub credentials_url: &'static str,
    /// What the allowance actually is, in the provider's own terms. Stated
    /// plainly rather than summed into a headline number, because these change
    /// often and a stale total would be a lie rather than an approximation.
    pub allowance: &'static str,
    /// What the key normally starts with, so a wrong paste is obvious.
    pub key_hint: Option<&'static str>,
    /// Which of this provider's models the free allowance covers.
    pub free_marker: FreeMarker,
    /// Whether this allowance renews, runs out, or is a shared endpoint.
    pub tier: FreeTier,
    /// What this provider may do with what it is sent.
    pub privacy: Privacy,
    /// How this endpoint departs from the plain OpenAI shape.
    pub quirks: Quirks,
    /// Where a *single model's* key is generated, when this provider issues
    /// keys per model rather than one key for the catalogue. `{model}` is
    /// replaced with the model id.
    ///
    /// Only NVIDIA has one, and finding out cost a confusing afternoon. A key
    /// from build.nvidia.com is described as unlocking "serverless APIs powered
    /// by NVIDIA NIM", which reads as all of them; in fact several models are
    /// provisioned individually, and one issued from a model's own page reaches
    /// that model and not the rest. Asking for an unprovisioned one answers 404
    /// with a message about an account, so it looks like a broken key rather
    /// than a missing entitlement.
    ///
    /// There is no way to discover this from the API — `/v1/models` lists every
    /// model whether or not the key reaches it — so the only honest thing Kuro
    /// can do is point at the page that fixes it.
    pub model_key_url: Option<&'static str>,
    pub models: &'static [FreeModel],
}

impl FreeProvider {
    /// Whether this provider can serve a request with what is on hand.
    ///
    /// One predicate, because "is this usable" stopped being the same question
    /// for every provider the moment keyless endpoints existed. Four places
    /// used to each write `keys.contains_key(slug)`, and four places would each
    /// have had to remember that a shared endpoint is different.
    pub fn is_reachable(&self, keys: &HashMap<String, String>, allow_keyless: bool) -> bool {
        if self.quirks.auth.keyless() {
            // A stored key still counts: on an endpoint that takes one
            // optionally, it usually buys a larger allowance.
            return allow_keyless || keys.contains_key(self.slug);
        }
        keys.contains_key(self.slug)
    }
}

/// The catalogue.
///
/// Ordered by how much a first key is worth: somebody adding exactly one should
/// add the first entry. Model ids are the ones each provider documents for its
/// free tier; a provider that renames one degrades to a failover rather than an
/// outage, because [`FreePool::choose`] checks every curated id against what the
/// provider currently advertises before asking for it.
///
/// Three entries were removed rather than repaired. GitHub Models retired the
/// endpoint this pointed at; Together's free tier turns out not to issue an API
/// key at all, so there was nothing to paste; and Chutes ended its free plan in
/// March 2026. A provider that cannot work is worse than an absent one, because
/// it is also an invitation to go looking for a key that will not help.
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
            // OpenRouter asks callers to identify themselves and uses it when
            // deciding rate limits.
            headers: &[
                ("HTTP-Referer", "https://github.com/Krysis-ux/kuro"),
                ("X-Title", "Kuro"),
            ],
            ..Quirks::OPENAI
        },
        model_key_url: None,
        models: &[FreeModel {
            // Every id curated here before had been retired, which is the
            // failure the live-catalogue check exists to survive. Kept short
            // deliberately: the pool reads the real list before it chooses.
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
            // Cohere refuses a tool whose schema carries `additionalProperties`
            // or `$schema`, and every Kuro built-in emits both.
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
        // Spelled out because the wording on NVIDIA's own page is actively
        // misleading. A key generated from the account page is described as
        // unlocking "serverless APIs powered by NVIDIA NIM", which reads as all
        // of them — and then several models answer 404 with a message about an
        // account, which reads as a broken key rather than a missing
        // entitlement. The fix is a second key from the model's own page, and
        // nothing anywhere says so.
        allowance: "Free API credits for developers, across a large catalogue. Note that one \
                    key does not reach every model: some are enabled per key. If a model here \
                    is greyed out, use the link on its row to generate a key from that model's \
                    own page, then paste it here.",
        key_hint: Some("nvapi-…"),
        free_marker: FreeMarker::Everything,
        tier: FreeTier::Recurring,
        privacy: Privacy::STANDARD,
        quirks: Quirks {
            // Some NVIDIA-hosted models serve one tool call at a time, and the
            // large ones can take minutes to produce a first token.
            no_parallel_tool_calls: true,
            timeout: Some(Duration::from_secs(180)),
            ..Quirks::OPENAI
        },
        // The one provider that issues keys per model. See the field's own
        // note: a general key does not reach every model here, and nothing in
        // the API says which ones it does reach.
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
            // Free.ai's chat route is `/chat/`, not `/chat/completions`.
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
    // ---- Shared endpoints, from here down --------------------------------
    //
    // No account, no key, and no promise. Their position at the bottom of this
    // list is not what keeps them last — `choose` sorts on keyless-ness before
    // anything else, so a keyed provider on its worst fallback still wins.
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

/// The rank a live-catalogue pick inherits.
///
/// The provider's own best curated rank, so a provider whose curated ids have
/// all been retired keeps its position in the list rather than sinking to the
/// bottom of it. That it is a worse *kind* of answer is already said by
/// `Quality::Live`, which sorts ahead of the rank.
fn model_rank(provider: &FreeProvider) -> u8 {
    provider
        .models
        .iter()
        .map(|model| model.rank)
        .min()
        .unwrap_or(u8::MAX)
}

/// Whether a model id names the free pool.
pub fn is_free_model(model_id: &str) -> bool {
    parse_selection(model_id).is_some()
}

/// The flavour a free model id asks for.
pub fn flavour_of(model_id: &str) -> Option<FreeFlavour> {
    match parse_selection(model_id)? {
        Selection::Flavour(flavour) => Some(flavour),
        Selection::Pinned { .. } => None,
    }
}

/// What a `free:` id is asking for.
///
/// Two shapes behind one prefix, because they are two genuinely different
/// requests and collapsing them lost one of them. `free:coding` is a
/// *preference* handed to the pool, which is free to satisfy it from whichever
/// provider still has allowance. `free:nvidia/meta/llama-3.3-70b-instruct` is a
/// *choice*: this provider, this model, and no failover to somewhere else that
/// would answer as a different model without saying so.
///
/// The second shape is what makes a provider's own catalogue selectable at all.
/// Before it, a key that reached sixty models could only ever be addressed
/// through four pooled rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Let the pool choose, preferring this flavour.
    Flavour(FreeFlavour),
    /// This provider's model, named exactly.
    Pinned {
        slug: String,
        /// The provider's own id for the model, which may itself contain `/`.
        model: String,
    },
}

/// The connector id a free provider's models are grouped under in the picker.
pub fn connector_id(slug: &str) -> String {
    format!("{MODEL_PREFIX}{slug}")
}

/// Read a `free:` model id.
///
/// Flavours are checked first, so a provider slug that happened to collide with
/// one could never shadow it. The model half is *not* split further: NVIDIA's
/// ids contain a slash of their own, and `meta/llama-3.3-70b-instruct` has to
/// survive the round trip intact.
pub fn parse_selection(model_id: &str) -> Option<Selection> {
    let rest = model_id.strip_prefix(MODEL_PREFIX)?;

    if let Some(flavour) = FreeFlavour::parse(rest) {
        return Some(Selection::Flavour(flavour));
    }

    let (slug, model) = rest.split_once('/')?;
    if model.is_empty() {
        return None;
    }
    // An unknown slug is not a free model. Answering otherwise would route a
    // typo into the pool and report it as an exhausted allowance.
    let provider = find(slug)?;

    Some(Selection::Pinned {
        slug: provider.slug.to_string(),
        model: model.to_string(),
    })
}

/// Why a provider is being skipped, and until when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trouble {
    /// Out of allowance for now.
    RateLimited,
    /// The key was rejected.
    Rejected,
    /// The model asked for is not there any more.
    ///
    /// Distinct from the other two because it is not about the key or the
    /// allowance — it is Kuro asking for something that no longer exists, which
    /// is Kuro's fault and is fixed by looking the catalogue up again rather
    /// than by waiting. So the cooldown is short: just long enough for the
    /// current message to fail over, after which a refreshed catalogue should
    /// make the provider usable again.
    Gone,
}

impl Trouble {
    /// How long something that refused this way is left alone.
    ///
    /// Public because the provider pools in [`crate::cloud`] apply the same
    /// cooldowns to individual free models. A 429 means the same thing whether
    /// it came from a provider or from one model on one — and two sets of
    /// numbers for the same idea would drift.
    pub fn cooldown(self) -> Duration {
        match self {
            Self::RateLimited => RATE_LIMIT_COOLDOWN,
            Self::Rejected => AUTH_COOLDOWN,
            Self::Gone => GONE_COOLDOWN,
        }
    }

    /// Read a provider's HTTP status as one of these, when it is one.
    pub fn from_status(status: u16) -> Option<Self> {
        match status {
            401 | 403 => Some(Self::Rejected),
            402 | 429 => Some(Self::RateLimited),
            // A provider answering "no such model" used to be read as no
            // trouble at all, so nothing was set aside and every following
            // message asked for the same missing model and failed the same way.
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

    /// Read back a stored kind. Unknown values are dropped rather than guessed.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "rate_limited" => Some(Self::RateLimited),
            "rejected" => Some(Self::Rejected),
            "model_gone" => Some(Self::Gone),
            _ => None,
        }
    }

    /// Whether this is a sign the cached catalogue is out of date.
    pub fn stale_catalogue(self) -> bool {
        self == Self::Gone
    }
}

/// One provider chosen for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub slug: String,
    pub name: String,
    pub base_url: String,
    /// The complete `Authorization` value, or `None` for an endpoint that takes
    /// none.
    ///
    /// Resolved here rather than by the request builder, so that "which literal
    /// does this endpoint want" is answered once, in the place that knows which
    /// endpoint it is. It was `api_key: String`, which obliged every caller to
    /// assume a bearer token — the assumption the shared endpoints break, and
    /// the one that would otherwise have sent `Bearer ` to a service that takes
    /// no key at all.
    pub authorization: Option<String>,
    pub model: String,
    /// How this endpoint departs from the plain OpenAI shape.
    pub quirks: Quirks,
}

/// How good an answer a candidate is, most preferred first.
///
/// Replaces an arithmetic penalty on the model's own rank. `rank + 50` and
/// `u8::MAX` conflated "a worse model" with "a worse *kind* of answer", and
/// there was no room left in a `u8` for a third axis once shared endpoints
/// arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Quality {
    /// A curated model that matches the flavour that was asked for.
    Matched,
    /// A curated model that does not, taken rather than refusing.
    AnyCurated,
    /// Whatever the provider currently advertises. The flavour is lost.
    Live,
}

/// Which providers are currently in trouble.
///
/// In memory rather than in the database: a cooldown is about the next few
/// minutes, and one that survived a restart would be describing a rate limit
/// that had long since reset.
#[derive(Clone, Default)]
pub struct FreePool {
    troubled: Arc<Mutex<HashMap<String, (Trouble, Instant)>>>,
    /// What each provider says it currently offers, and when it said so.
    ///
    /// The catalogue below is hand-written, which is the only way to know that
    /// one model is a coder and another reasons — a provider's model list does
    /// not say. But hand-written means it goes stale, and it did: every
    /// OpenRouter id here was retired, so Kuro Free answered every message with
    /// a 404 while the same key worked perfectly through the provider screen,
    /// which reads the live list.
    ///
    /// So the curated table now proposes and the live list disposes. Kuro only
    /// ever asks for a model the provider is currently advertising, and a
    /// retirement costs a failover instead of an outage.
    live: Arc<Mutex<Catalogues>>,
    /// Whether shared, unauthenticated endpoints may answer.
    ///
    /// Mirrored here from the settings table for the same reason the cooldowns
    /// live here: [`FreePool::choose`] must not read the database, and a
    /// request that has already started must not wait on one.
    ///
    /// `false` when this struct is default-constructed, which is what keeps a
    /// pool built in a test — or before settings have been read — behaving
    /// exactly as it did before shared endpoints existed.
    allow_keyless: Arc<AtomicBool>,
    /// Models a provider advertises but will not actually serve on this key.
    ///
    /// Separate from `troubled`, which is about the provider, because NVIDIA NIM
    /// proved the two are not the same thing. A key from build.nvidia.com does
    /// not reach every model in the catalogue: several are provisioned
    /// per-model, and asking for one the key was not issued for answers
    ///
    /// ```text
    /// 404 {"detail":"Function '0417…': Not found for account 'p4OU…'"}
    /// ```
    ///
    /// which is about that model and that key, and says nothing at all about
    /// the provider. Reading it as a provider failure — which is what happened —
    /// set NVIDIA aside entirely and threw away its catalogue, so one
    /// unprovisioned model took out eighty-two working ones and the next
    /// message reported that there was no working NVIDIA key.
    ///
    /// Keyed on `slug/model`, and short-lived, because the fix is the user
    /// generating a key and coming back — which should start working
    /// immediately rather than after a long cooldown.
    unavailable: Arc<Mutex<HashMap<String, (Trouble, Instant)>>>,
    /// Whether a catalogue read is already running.
    ///
    /// Reading every provider's model list takes as long as the slowest of
    /// them, and it now happens in the background rather than in front of the
    /// first token. That trade only works if a burst of messages starts one
    /// read rather than one each: twenty concurrent reads of the same twenty
    /// endpoints is a worse stall than the one being avoided, and it arrives
    /// exactly when the user is most active.
    refreshing: Arc<AtomicBool>,
}

/// What each provider advertises, and when it was asked.
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

    /// Note that a provider refused, so the next request skips it.
    pub fn note_trouble(&self, slug: &str, trouble: Trouble) {
        self.troubled
            .lock()
            .expect("free pool lock")
            .insert(slug.to_string(), (trouble, Instant::now()));
        tracing::info!(slug, kind = trouble.as_str(), "free provider set aside");
    }

    /// Forget a provider's trouble, after a key is replaced or a test succeeds.
    ///
    /// Clears the per-model refusals too. A new key is exactly the thing that
    /// changes which models are reachable, so holding on to "this one 404'd"
    /// across a key change would hide the fix the user just applied.
    pub fn clear_trouble(&self, slug: &str) {
        self.troubled.lock().expect("free pool lock").remove(slug);

        let prefix = format!("{slug}/");
        self.unavailable
            .lock()
            .expect("free pool lock")
            .retain(|key, _| !key.starts_with(&prefix));
    }

    /// Note that one model will not serve on this key.
    pub fn note_model_trouble(&self, slug: &str, model: &str, trouble: Trouble) {
        self.unavailable
            .lock()
            .expect("free pool lock")
            .insert(format!("{slug}/{model}"), (trouble, Instant::now()));
        tracing::info!(slug, model, kind = trouble.as_str(), "free model set aside");
    }

    /// Why one model is currently being skipped, if it is.
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

    /// Allow or forbid the shared, unauthenticated endpoints.
    pub fn set_allow_keyless(&self, allowed: bool) {
        self.allow_keyless.store(allowed, Ordering::Relaxed);
    }

    /// Whether shared endpoints may currently answer.
    pub fn allows_keyless(&self) -> bool {
        self.allow_keyless.load(Ordering::Relaxed)
    }

    /// Record what a provider says it currently offers.
    pub fn set_live_models(&self, slug: &str, models: Vec<String>) {
        self.live
            .lock()
            .expect("free pool lock")
            .insert(slug.to_string(), (models, Instant::now()));
    }

    /// Providers and models currently set aside, with how long is left.
    ///
    /// Seconds remaining rather than a timestamp, because [`Instant`] is not a
    /// wall clock and cannot be stored — and seconds is the honest unit anyway:
    /// what survives a restart should be "this had four minutes left", not a
    /// clock reading that a suspended laptop makes meaningless.
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

    /// Put back what was set aside before a restart.
    ///
    /// Without this, restarting cleared every refusal — so a key the provider
    /// had rejected came back looking fine, was tried again, failed again, and
    /// the picker showed it as available throughout. A cooldown that a restart
    /// erases is not a cooldown.
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
            // Backdated so it expires when it would have, rather than starting
            // its cooldown again from zero on every restart.
            let Some(since) = now.checked_sub(elapsed) else {
                continue;
            };

            // A `/` means it names one model on a provider rather than the
            // provider itself — the same split `note_model_trouble` writes.
            let target = if key.contains('/') {
                &self.unavailable
            } else {
                &self.troubled
            };
            target.lock().expect("free pool lock").insert(key, (trouble, since));
        }
    }

    /// Every catalogue currently held, for writing to storage.
    pub fn catalogues(&self) -> HashMap<String, Vec<String>> {
        self.live
            .lock()
            .expect("free pool lock")
            .iter()
            .map(|(slug, (models, _))| (slug.clone(), models.clone()))
            .collect()
    }

    /// Put back catalogues read before a restart.
    ///
    /// Marked as read *now* rather than when they were actually fetched, which
    /// is the deliberate choice: the alternative is storing a wall-clock time
    /// and comparing it against [`Instant`], which cannot be done, and the
    /// consequence of being generous is at worst one stale id and a failover.
    ///
    /// The reason this exists at all is a cold start showing a lie. A cloud
    /// connector's models live in the database, so OpenRouter's four hundred
    /// were there the moment the picker opened; a free provider's lived only in
    /// this process, so every one of them was missing until a background read
    /// finished — and the picker had already rendered by then. The result was a
    /// model list that showed one provider out of five and gave no sign it was
    /// still filling in.
    pub fn restore_catalogues(&self, stored: HashMap<String, Vec<String>>) {
        let mut held = self.live.lock().expect("free pool lock");
        let now = Instant::now();
        for (slug, models) in stored {
            // Anything read during this process is fresher by definition.
            held.entry(slug).or_insert((models, now));
        }
    }

    /// Forget a provider's catalogue, so the next request reads it again.
    pub fn forget_live_models(&self, slug: &str) {
        self.live.lock().expect("free pool lock").remove(slug);
    }

    /// Claim the right to read the catalogues, if nobody else holds it.
    ///
    /// `true` means this caller should do the read and must call
    /// [`FreePool::end_refresh`] when it finishes. `false` means one is already
    /// running and this caller should carry on with what is cached.
    pub fn begin_refresh(&self) -> bool {
        self.refreshing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Release the claim taken by [`FreePool::begin_refresh`].
    pub fn end_refresh(&self) {
        self.refreshing.store(false, Ordering::Release);
    }

    /// Whether any reachable provider's catalogue is worth re-reading.
    ///
    /// Asked before spawning a background read, so the common case — every
    /// catalogue fresh — costs a few comparisons rather than a task.
    pub fn needs_any_catalogue(&self, keys: &HashMap<String, String>) -> bool {
        let allow_keyless = self.allows_keyless();
        FREE_PROVIDERS
            .iter()
            .filter(|provider| provider.is_reachable(keys, allow_keyless))
            .any(|provider| self.needs_catalogue(provider.slug))
    }

    /// Whether this provider's catalogue is missing or too old to trust.
    pub fn needs_catalogue(&self, slug: &str) -> bool {
        self.live
            .lock()
            .expect("free pool lock")
            .get(slug)
            .is_none_or(|(_, read_at)| read_at.elapsed() >= CATALOGUE_TTL)
    }

    /// Whether a provider currently advertises a model.
    ///
    /// Unknown catalogues answer `true`: a provider Kuro has not managed to ask
    /// yet should be tried with its curated models rather than treated as
    /// offering nothing, which would strand a working key behind a failed
    /// catalogue read.
    fn advertises(&self, slug: &str, model: &str) -> bool {
        let held = self.live.lock().expect("free pool lock");
        let Some((models, _)) = held.get(slug) else {
            return true;
        };
        models.is_empty() || models.iter().any(|known| known == model)
    }

    /// The trouble a provider is currently in, if its cooldown has not expired.
    pub fn trouble(&self, slug: &str) -> Option<Trouble> {
        let mut held = self.troubled.lock().expect("free pool lock");
        let (trouble, since) = *held.get(slug)?;
        if since.elapsed() >= trouble.cooldown() {
            held.remove(slug);
            return None;
        }
        Some(trouble)
    }

    /// Pick a provider and model for one request.
    ///
    /// `keys` is what the caller found in the credential store, so this stays
    /// free of I/O and therefore testable. The order is: models that match the
    /// asked-for flavour, best rank first; then, if none matched, anything the
    /// user has a key for, so a request never fails merely because the flavour
    /// was specific.
    pub fn choose(&self, flavour: FreeFlavour, keys: &HashMap<String, String>) -> Option<Choice> {
        let allow_keyless = self.allows_keyless();
        let usable = |provider: &FreeProvider| {
            provider.is_reachable(keys, allow_keyless) && self.trouble(provider.slug).is_none()
        };

        // Every candidate, gathered in one pass and ranked by the sort below
        // rather than by which loop found it.
        let mut candidates: Vec<(bool, Quality, u8, &FreeProvider, String)> = Vec::new();

        for provider in FREE_PROVIDERS.iter().filter(|held| usable(held)) {
            let keyless = provider.quirks.auth.keyless() && !keys.contains_key(provider.slug);

            for model in provider.models {
                if !self.advertises(provider.slug, model.id) {
                    continue;
                }
                // Refused on this key specifically. Skipping it here is what
                // turns "one model is not provisioned" into a failover rather
                // than into a provider that appears to be down.
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
                    // Falling back to any model at all is deliberate. Somebody
                    // who added one key and asked for `free:coding` is better
                    // served by a general model than by an error explaining
                    // that their single provider is not a coder.
                    candidates.push((
                        keyless,
                        Quality::AnyCurated,
                        model.rank,
                        provider,
                        model.id.to_string(),
                    ));
                }
            }

            // And if the curated list has been overtaken entirely — every id
            // Kuro knows about retired, which is what happened to OpenRouter —
            // take whatever the provider is advertising. The flavour survives
            // this now: the live list is read rather than merely counted, so a
            // provider Kuro has no hand-written entry for can still answer
            // `free:coding` with a coder.
            if let Some(model) = self.best_live_model(provider, flavour) {
                candidates.push((keyless, Quality::Live, model_rank(provider), provider, model));
            }
        }

        // Keyless-ness is the *most* significant key, so a provider the user
        // holds a key for still wins on its worst fallback against a shared
        // endpoint's best match. Sorting rather than early-exiting per tier is
        // what makes that expressible at all.
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

    /// The best *free* model a provider advertises, when nothing curated survives.
    ///
    /// Three filters, in the order that matters.
    ///
    /// The provider's marker comes first, because it is the line between a
    /// fallback and a bill: OpenRouter advertises three hundred and thirty-seven
    /// models to this key and fourteen of them are free.
    ///
    /// Then [`classify`], because a provider's model list is not a list of chat
    /// models. NVIDIA's sixty-odd entries include embedding models, rerankers,
    /// speech recognisers and safety classifiers, and this used to sort the lot
    /// alphabetically and take the first — which on that catalogue is not a
    /// model you can hold a conversation with. Every message routed there failed,
    /// and the failure looked like the key was wrong.
    ///
    /// Then the flavour, which this could not honour at all before. A provider
    /// with no hand-written entry can now still answer `free:coding` with
    /// something named like a coder, instead of losing the request's intent
    /// the moment the curated table runs out.
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
                // A model that refused is deliberately still listed. Hiding it
                // would answer "why can't I find the model I was using
                // yesterday" with silence; greying it out and saying why is the
                // whole point. Routing skips it separately, in `best_live_model`.
                // A tiny model is a poor answer to "whatever is best", so on
                // `Auto` the preference runs the other way.
                let matches = match wanted {
                    Some(speciality) => classified.has(speciality),
                    None => !classified.has(Speciality::Fast),
                };
                Some((!matches, model))
            })
            .collect();

        // Sorted on the match first and the name second, so the same key picks
        // the same model across restarts rather than whichever one the provider
        // happened to list first today.
        usable.sort();
        usable.first().map(|(_, model)| (*model).clone())
    }

    /// Route to one named provider and model, with no failover.
    ///
    /// The absence of failover is the point rather than a shortcoming. Somebody
    /// who picked a model by name asked for that model; quietly answering as a
    /// different one from a different company because the first was rate-limited
    /// is the behaviour the flavour rows exist to provide, and doing it here
    /// would make the two indistinguishable.
    ///
    /// A cooldown is still honoured, because sending a request to a key that
    /// was rejected two minutes ago is not failover — it is a round trip whose
    /// answer is already known.
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

    /// Every chat model a provider currently advertises inside its allowance.
    ///
    /// For the picker, which wants the whole list rather than one choice from
    /// it: somebody who has added an NVIDIA key wants to see what that key
    /// reaches, and until now the only thing Kuro would show them was four
    /// pooled rows that hid every one of them.
    ///
    /// Empty when the catalogue has not been read yet. That is deliberately
    /// different from [`FreePool::advertises`], which treats unknown as "try
    /// it": proposing a model is a guess worth making, and listing one under a
    /// heading that says these are available is a claim.
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

    /// Providers that could serve a request right now.
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
        // A provider nobody can get a key for is not an option, and an allowance
        // nobody can see is a number somebody will assume is unlimited.
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
        // The honest failure. Kuro supplies no keys, and a pool that answered
        // anyway would be answering from somewhere nobody agreed to.
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

        // Ollama Cloud curates one general model and one code model, which is
        // the whole point of the hand-written table: a model list does not say
        // which of its entries is a coder.
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
        // Somebody with a single Cloudflare key asking for `free:reasoning` is
        // better served by the model they have than by an explanation.
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
        // The bug this exists for. Every OpenRouter id in the table above was
        // retired, so Kuro Free answered every message with
        // "This model is unavailable for free" while the same key worked through
        // the provider screen, which reads the live list.
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
        // Live discovery must not throw away the one thing the hand-written
        // table is for: knowing which model is a coder.
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
        // A failed catalogue read must not strand a working key.
        let pool = FreePool::new();
        assert!(pool.needs_catalogue("groq"));
        assert!(pool.choose(FreeFlavour::Auto, &keys(&["groq"])).is_some());
    }

    #[test]
    fn an_empty_catalogue_is_treated_as_unknown_rather_than_as_nothing_offered() {
        // Some endpoints answer `/models` with an empty list or a shape Kuro
        // cannot read. That is not the same as "this provider has no models",
        // and reading it that way would disable a key that works.
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
        // Caught in testing: with every curated OpenRouter id retired, the
        // fallback took the alphabetically first of all 337 models the key could
        // reach — `ai21/jamba-large-1.7`, which is paid. Kuro Free spending real
        // money is the one outcome this whole screen exists to rule out.
        let pool = FreePool::new();
        pool.set_live_models(
            "openrouter",
            vec![
                "ai21/jamba-large-1.7".to_string(),
                "anthropic/claude-opus-5".to_string(),
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
        pool.set_live_models("openrouter", vec!["anthropic/claude-opus-5".to_string()]);

        assert!(
            pool.choose(FreeFlavour::Auto, &keys(&["openrouter"])).is_none(),
            "no free model is not the same as any model"
        );
    }

    #[test]
    fn an_account_level_free_tier_may_use_anything_it_lists() {
        // Groq's free tier has no card attached, so nothing it can reach bills.
        let pool = FreePool::new();
        pool.set_live_models("groq", vec!["some-new-groq-model".to_string()]);

        let chosen = pool.choose(FreeFlavour::Auto, &keys(&["groq"])).expect("a model");
        assert_eq!(chosen.model, "some-new-groq-model");
    }

    /// A slice of NVIDIA NIM's catalogue, in the shape it actually arrives in.
    ///
    /// The point of the sample is that most of it cannot hold a conversation.
    /// `/v1/models` on that endpoint is not a list of chat models — it is every
    /// model the platform hosts, and the embedding models, rerankers, speech
    /// recognisers and safety classifiers sit in it unlabelled beside the
    /// Llamas.
    fn nvidia_catalogue() -> Vec<String> {
        [
            // Alphabetically first, and an embedding model.
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
        // The NVIDIA bug. The fallback sorted the advertised ids and took the
        // first, which on this catalogue is `baai/bge-m3` — an embedding model
        // that answers a conversation with a 400. Every message routed to
        // NVIDIA failed, and the failure read as a bad key.
        let pool = FreePool::new();
        pool.set_live_models("nvidia", nvidia_catalogue());

        // Curated ids removed from the advertised list, so only the fallback
        // path can answer. This is the state a provider rename leaves behind.
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
        // It used to lose it: the fallback took one model per provider and the
        // request's intent went with it, so `free:coding` on a provider with no
        // hand-written entry got whatever sorted first.
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
        // Reading the catalogues moved off the path the user waits on, which
        // only helps if a burst of messages starts one sweep rather than one
        // each. Twenty concurrent reads of the same twenty endpoints would be a
        // worse stall than the one being avoided, arriving exactly when the
        // user is most active.
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
        // The provider's own id contains a slash, so a naive split would hand
        // the pool `meta` and ask NVIDIA for `llama-3.3-70b-instruct` — a model
        // it does not have under that name.
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
        // A pinned id has no flavour: it named a model rather than a preference.
        assert_eq!(flavour_of("free:nvidia/meta/llama-3.3-70b-instruct"), None);
    }

    #[test]
    fn an_id_naming_a_provider_kuro_does_not_have_is_not_a_free_model() {
        // Otherwise a typo routes into the pool and comes back reported as an
        // exhausted allowance, which sends somebody to check a key that is fine.
        assert_eq!(parse_selection("free:notaprovider/some-model"), None);
        assert!(!is_free_model("free:notaprovider/some-model"));
        assert!(!is_free_model("free:nvidia/"));
        assert!(!is_free_model("free:nvidia"));
    }

    #[test]
    fn one_model_refusing_does_not_take_the_provider_down_with_it() {
        // The NVIDIA case. A key that is not provisioned for one model answers
        // 404 for that model and works perfectly for the other eighty-two, and
        // reading it as a provider failure meant one unprovisioned model
        // reported that there was no working NVIDIA key at all.
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
        // A cooldown a restart erases is not a cooldown: the rejected key came
        // back looking fine, was tried again, failed again, and the picker
        // offered it as available throughout.
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
        // And the split between the two maps survives the round trip: a
        // provider-level refusal must not come back as a model-level one.
        assert!(after.trouble("nvidia").is_none());
    }

    #[test]
    fn a_refusal_that_has_already_expired_is_not_restored() {
        let pool = FreePool::new();
        // Zero seconds left is an expired cooldown, and restoring it would set
        // a provider aside for a full fresh cooldown on every restart.
        pool.restore_troubles(vec![("groq".to_string(), "rate_limited".to_string(), 0)]);

        assert!(pool.trouble("groq").is_none());
    }

    #[test]
    fn replacing_a_key_clears_the_models_it_had_refused() {
        // A new key is exactly the thing that changes which models are
        // reachable, so holding on to "this one 404'd" would hide the fix the
        // user just applied.
        let pool = FreePool::new();
        pool.note_model_trouble("nvidia", "some/model", Trouble::Gone);
        pool.note_trouble("nvidia", Trouble::Rejected);

        pool.clear_trouble("nvidia");

        assert!(pool.trouble("nvidia").is_none());
        assert!(pool.model_trouble("nvidia", "some/model").is_none());
    }

    #[test]
    fn a_pinned_choice_does_not_fail_over_to_another_company() {
        // The whole difference between picking `Kuro Free · coding` and picking
        // a model by name. One is a preference, the other is a choice, and a
        // choice that silently answered as a different vendor's model would
        // make the two indistinguishable.
        let pool = FreePool::new();
        let chosen = pool
            .pinned("nvidia", "meta/llama-3.3-70b-instruct", &keys(&["nvidia", "groq"]))
            .expect("a key for nvidia");

        assert_eq!(chosen.slug, "nvidia");
        assert_eq!(chosen.model, "meta/llama-3.3-70b-instruct");

        // With no key for that provider it refuses rather than borrowing one.
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
        // Distinct from `advertises`, which treats unknown as "worth trying".
        // Proposing a model is a guess; listing one under a heading saying the
        // key reaches it is a claim.
        let pool = FreePool::new();
        assert!(pool
            .advertised_chat_models(find("nvidia").expect("nvidia"))
            .is_empty());
    }

    #[test]
    fn a_guard_model_is_never_offered_as_a_conversation() {
        // `meta/llama-guard-4-12b` sorts before `meta/llama-3.3-70b`? No — but
        // it is a Llama, it is free, and nothing but the classifier keeps it
        // out of a failover.
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
        // The markers and the hand-written ids have to agree, or the filter that
        // protects the wallet would also filter out the curated list.
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

        // Both serve a catalogue where most models are billed and a handful are
        // not, so only the marked ones may be used. Everything else in the table
        // is an account that cannot be billed at all.
        assert_eq!(by_suffix, vec!["openrouter", "kilo"]);

        assert!(FreeMarker::Suffix(&[":free"]).covers("a/b:free"));
        assert!(!FreeMarker::Suffix(&[":free"]).covers("a/b"));
        assert!(FreeMarker::Everything.covers("anything-at-all"));
    }

    #[test]
    fn a_marker_can_carry_more_than_one_convention() {
        // Kilo marks both `…:free` and its automatic router `kilo-auto/free`.
        let kilo = find("kilo").expect("kilo");

        assert!(kilo.free_marker.covers("inclusionai/ling-3.0-flash:free"));
        assert!(kilo.free_marker.covers("kilo-auto/free"));
        assert!(!kilo.free_marker.covers("anthropic/claude-opus-5"));
    }

    #[test]
    fn the_retired_providers_are_gone_and_do_not_resolve() {
        // GitHub Models retired its endpoint, Together's free tier issues no
        // key, and Chutes ended its free plan. Each was answering every request
        // with an error.
        for slug in ["github", "together", "chutes"] {
            assert!(find(slug).is_none(), "`{slug}` is still in the table");
        }
    }

    #[test]
    fn every_keyless_provider_says_it_is_shared() {
        // A shared endpoint that did not warn would be the one place in Kuro
        // where a prompt leaves for somewhere the user never chose.
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
        // The setting is off by default, so a pool built before settings have
        // been read behaves exactly as it did before shared endpoints existed.
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
        // The whole point of ranking keyless-ness above quality: a provider the
        // user actually holds a key for, reduced to a live-catalogue guess with
        // the flavour lost, still beats a shared endpoint's exact match.
        let pool = FreePool::new();
        pool.set_allow_keyless(true);

        // Cerebras keeps its key but every curated id it has is retired, so the
        // only thing left for it is a live pick.
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

        // Every keyed provider the user holds is out of allowance.
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
        // Some shared endpoints take an optional key that buys a bigger
        // allowance. Holding one makes that provider a keyed provider.
        let pool = FreePool::new();
        // Deliberately NOT allowing keyless: a stored key is its own permission.
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
        // 404 used to mean "no trouble", so nothing was set aside and every
        // following message asked the same provider for the same dead model.
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
        // A 429 resets on its own; a 401 means the key is wrong, and retrying it
        // every three minutes forever helps nobody.
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
