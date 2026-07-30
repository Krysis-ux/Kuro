//! The providers Kuro knows the shape of.
//!
//! A preset saves the user finding a base URL, and nothing else — there is no
//! per-provider code path. Anything not listed here still works through
//! `custom`, which is the point: the list is a convenience, not a gate.
//!
//! Each entry says what it is for, because "add a provider" is only a useful
//! screen if it also answers "which one".

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    /// One key, many companies' models behind it.
    Aggregator,
    /// A model developer's own API.
    FirstParty,
    /// Rented hardware running an OpenAI-compatible server.
    RentedGpu,
    /// Anything else that speaks the OpenAI API.
    Custom,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    pub slug: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub blurb: &'static str,
    pub kind: PresetKind,
    /// Where the key comes from.
    pub credentials_url: Option<&'static str>,
    /// What the key normally starts with, shown as a placeholder so a wrong paste
    /// is obvious before the request is made.
    pub key_hint: Option<&'static str>,
    /// Whether the base URL is the user's to supply.
    pub needs_url: bool,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        slug: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        blurb: "One key for models from Anthropic, OpenAI, Google, Meta and most others. \
                The simplest answer if you want everything at once.",
        kind: PresetKind::Aggregator,
        credentials_url: Some("https://openrouter.ai/keys"),
        key_hint: Some("sk-or-v1-…"),
        needs_url: false,
    },
    Preset {
        slug: "anthropic",
        name: "Anthropic",
        base_url: "https://api.anthropic.com/v1",
        blurb: "Claude, direct from Anthropic, through their OpenAI-compatible endpoint.",
        kind: PresetKind::FirstParty,
        credentials_url: Some("https://console.anthropic.com/settings/keys"),
        key_hint: Some("sk-ant-…"),
        needs_url: false,
    },
    Preset {
        slug: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        blurb: "GPT models, direct from OpenAI.",
        kind: PresetKind::FirstParty,
        credentials_url: Some("https://platform.openai.com/api-keys"),
        key_hint: Some("sk-…"),
        needs_url: false,
    },
    Preset {
        slug: "groq",
        name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        blurb: "Open-weight models served very fast. Useful when latency matters more than choice.",
        kind: PresetKind::FirstParty,
        credentials_url: Some("https://console.groq.com/keys"),
        key_hint: Some("gsk_…"),
        needs_url: false,
    },
    Preset {
        slug: "deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com/v1",
        blurb: "DeepSeek's own API, including their reasoning models.",
        kind: PresetKind::FirstParty,
        credentials_url: Some("https://platform.deepseek.com/api_keys"),
        key_hint: Some("sk-…"),
        needs_url: false,
    },
    Preset {
        slug: "mistral",
        name: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        blurb: "Mistral's hosted models, including the small ones worth comparing against \
                what runs locally.",
        kind: PresetKind::FirstParty,
        credentials_url: Some("https://console.mistral.ai/api-keys"),
        key_hint: None,
        needs_url: false,
    },
    Preset {
        slug: "together",
        name: "Together",
        base_url: "https://api.together.xyz/v1",
        blurb: "A large catalogue of open-weight models, including ones too big to run here.",
        kind: PresetKind::Aggregator,
        credentials_url: Some("https://api.together.ai/settings/api-keys"),
        key_hint: None,
        needs_url: false,
    },
    Preset {
        slug: "runpod",
        name: "RunPod",
        base_url: "",
        blurb: "A GPU you rented, running vLLM or llama.cpp. Paste the endpoint URL your pod \
                exposes — usually ending in `/v1`.",
        kind: PresetKind::RentedGpu,
        credentials_url: Some("https://www.runpod.io/console/user/settings"),
        key_hint: None,
        needs_url: true,
    },
    Preset {
        slug: "vastai",
        name: "Vast.ai",
        base_url: "",
        blurb: "Same idea as RunPod: your instance, your endpoint, your bill.",
        kind: PresetKind::RentedGpu,
        credentials_url: Some("https://cloud.vast.ai/manage-keys/"),
        key_hint: None,
        needs_url: true,
    },
    Preset {
        slug: "custom",
        name: "Custom endpoint",
        base_url: "",
        blurb: "Anything that speaks the OpenAI API — another machine on your network, \
                a company gateway, a self-hosted server.",
        kind: PresetKind::Custom,
        credentials_url: None,
        key_hint: None,
        needs_url: true,
    },
];

pub fn find(slug: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.slug == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slug_is_unique_and_findable() {
        let mut seen: Vec<&str> = Vec::new();
        for preset in PRESETS {
            assert!(!seen.contains(&preset.slug), "duplicate slug `{}`", preset.slug);
            seen.push(preset.slug);
            assert!(find(preset.slug).is_some());
        }
        assert!(find("bogus").is_none());
    }

    #[test]
    fn a_preset_either_supplies_a_url_or_asks_for_one() {
        for preset in PRESETS {
            if preset.needs_url {
                assert!(
                    preset.base_url.is_empty(),
                    "`{}` asks for a URL but also hardcodes one",
                    preset.slug
                );
            } else {
                assert!(
                    preset.base_url.starts_with("https://"),
                    "`{}` should carry an https base URL",
                    preset.slug
                );
            }
        }
    }

    #[test]
    fn hosted_presets_point_at_their_own_key_page() {
        for preset in PRESETS.iter().filter(|p| p.kind == PresetKind::FirstParty) {
            assert!(
                preset.credentials_url.is_some(),
                "`{}` should say where its key comes from",
                preset.slug
            );
        }
    }

    #[test]
    fn every_preset_explains_what_it_is_for() {
        for preset in PRESETS {
            assert!(preset.blurb.len() > 30, "`{}` needs a real description", preset.slug);
            assert!(!preset.name.is_empty());
        }
    }

    #[test]
    fn custom_is_present_so_nothing_is_locked_out() {
        let custom = find("custom").expect("custom");
        assert_eq!(custom.kind, PresetKind::Custom);
        assert!(custom.needs_url);
    }

    #[test]
    fn base_urls_do_not_include_the_completions_path() {
        for preset in PRESETS {
            assert!(
                !preset.base_url.contains("chat/completions"),
                "`{}` should be a base URL, not an endpoint",
                preset.slug
            );
        }
    }
}
