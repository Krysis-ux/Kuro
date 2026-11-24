//! How an endpoint departs from the plain OpenAI shape. Per-provider differences
//! live here as data instead of a `match` in the streaming code.
//!
//! This is separate from [`crate::free`] because a quirk belongs to an endpoint,
//! not a free tier, and [`crate::cloud`] should not depend on the free pool.
//! [`Quirks::OPENAI`] is the default for endpoints that need nothing special.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

/// How to authenticate, and whether a key is needed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Auth {
    /// `Authorization: Bearer <the user's key>`.
    Bearer,
    /// No `Authorization` header at all (Kilo, OVH). An empty bearer would be
    /// read as a malformed key, so the header must be absent.
    None,
    /// A placeholder the endpoint requires but does not check, used when the user
    /// has no key. A stored real key is sent instead (usually a larger allowance).
    Anonymous(&'static str),
}

impl Auth {
    /// Whether this endpoint can answer with nothing stored for it.
    pub fn keyless(self) -> bool {
        !matches!(self, Self::Bearer)
    }

    /// The `Authorization` value to send, or `None` for no header. `Bearer` with
    /// no key yields `None` rather than an empty bearer, which would only show up
    /// as a confusing 401.
    pub fn authorization(self, stored: Option<&str>) -> Option<String> {
        let stored = stored.map(str::trim).filter(|key| !key.is_empty());
        match (self, stored) {
            (Self::None, _) => None,
            (Self::Bearer | Self::Anonymous(_), Some(key)) => Some(format!("Bearer {key}")),
            (Self::Bearer, None) => None,
            (Self::Anonymous(literal), None) => Some(format!("Bearer {literal}")),
        }
    }
}

/// What one endpoint needs that the plain OpenAI shape does not describe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Quirks {
    pub auth: Auth,
    /// Appended to the base URL for a chat request.
    pub chat_path: &'static str,
    /// Whether to remove the JSON Schema keywords some providers reject.
    pub strip_schema_keywords: bool,
    /// Whether to say `parallel_tool_calls: false` explicitly.
    pub no_parallel_tool_calls: bool,
    /// Sent on every request. Some gateways ask for attribution headers.
    pub headers: &'static [(&'static str, &'static str)],
    /// A cap for this endpoint, for providers that can take minutes to start.
    pub timeout: Option<Duration>,
}

impl Quirks {
    /// Plain OpenAI behaviour.
    pub const OPENAI: Self = Self {
        auth: Auth::Bearer,
        chat_path: "/chat/completions",
        strip_schema_keywords: false,
        no_parallel_tool_calls: false,
        headers: &[],
        timeout: None,
    };

    /// Where a chat request for this endpoint goes.
    pub fn chat_url(&self, base_url: &str) -> String {
        format!("{}{}", base_url.trim_end_matches('/'), self.chat_path)
    }
}

impl Default for Quirks {
    fn default() -> Self {
        Self::OPENAI
    }
}

/// JSON Schema keywords that some providers reject outright.
const REJECTED_KEYWORDS: &[&str] = &["additionalProperties", "$schema"];

/// Remove the keywords a provider rejects (Cohere rejects `additionalProperties`
/// and `$schema`, which every built-in tool emits) at every depth.
///
/// Only the schema's structural keywords are followed, so a tool *property* that
/// happens to be named `$schema` is kept.
pub fn strip_schema_keywords(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                strip_schema_keywords(item);
            }
        }
        Value::Object(map) => {
            for keyword in REJECTED_KEYWORDS {
                map.remove(*keyword);
            }

            // `properties` maps names to schemas: recurse into values only.
            if let Some(Value::Object(properties)) = map.get_mut("properties") {
                for schema in properties.values_mut() {
                    strip_schema_keywords(schema);
                }
            }

            // Keywords whose values are schemas.
            for nested in ["items", "additionalItems", "not", "if", "then", "else"] {
                if let Some(schema) = map.get_mut(nested) {
                    strip_schema_keywords(schema);
                }
            }

            // Keywords whose values are arrays of schemas.
            for nested in ["allOf", "anyOf", "oneOf", "prefixItems"] {
                if let Some(Value::Array(schemas)) = map.get_mut(nested) {
                    for schema in schemas {
                        strip_schema_keywords(schema);
                    }
                }
            }

            // Tool-definition wrappers and schema definition tables.
            for nested in ["function", "parameters", "$defs", "definitions"] {
                if let Some(schema) = map.get_mut(nested) {
                    strip_schema_keywords(schema);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_bearer_endpoint_sends_the_key_and_nothing_without_one() {
        assert_eq!(
            Auth::Bearer.authorization(Some("sk-test")),
            Some("Bearer sk-test".to_string())
        );
        assert_eq!(Auth::Bearer.authorization(None), None);
        assert!(!Auth::Bearer.keyless());
    }

    #[test]
    fn a_keyless_endpoint_never_gets_an_authorization_header() {
        // Never send `Bearer ` or someone else's key to a shared anonymous endpoint.
        assert_eq!(Auth::None.authorization(None), None);
        assert_eq!(
            Auth::None.authorization(Some("sk-somebody-elses-key")),
            None,
            "an endpoint that takes no key must not be handed one"
        );
        assert!(Auth::None.keyless());
    }

    #[test]
    fn an_anonymous_endpoint_uses_its_literal_until_a_real_key_exists() {
        let anonymous = Auth::Anonymous("unused");

        assert_eq!(anonymous.authorization(None), Some("Bearer unused".to_string()));
        assert_eq!(
            anonymous.authorization(Some("real-key")),
            Some("Bearer real-key".to_string()),
            "a real key buys a larger allowance and must win"
        );
        assert!(anonymous.keyless());
    }

    #[test]
    fn a_blank_stored_key_is_treated_as_no_key_at_all() {
        assert_eq!(Auth::Bearer.authorization(Some("   ")), None);
        assert_eq!(
            Auth::Anonymous("unused").authorization(Some("")),
            Some("Bearer unused".to_string())
        );
    }

    #[test]
    fn the_default_is_what_every_provider_did_before_this_existed() {
        let quirks = Quirks::default();

        assert_eq!(quirks, Quirks::OPENAI);
        assert_eq!(quirks.auth, Auth::Bearer);
        assert_eq!(
            quirks.chat_url("https://api.example.com/v1"),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn a_trailing_slash_on_the_base_url_does_not_double_up() {
        assert_eq!(
            Quirks::OPENAI.chat_url("https://api.example.com/v1/"),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn an_endpoint_can_name_its_own_chat_path() {
        let quirks = Quirks {
            chat_path: "/chat/",
            ..Quirks::OPENAI
        };

        assert_eq!(quirks.chat_url("https://api.free.ai/v1"), "https://api.free.ai/v1/chat/");
    }

    #[test]
    fn rejected_keywords_are_removed_at_every_depth() {
        let mut schema = json!({
            "type": "object",
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "additionalProperties": false,
            "properties": {
                "path": { "type": "string" },
                "nested": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": { "deep": { "type": "string" } }
                }
            },
            "items": { "type": "object", "additionalProperties": true }
        });

        strip_schema_keywords(&mut schema);

        assert!(schema.get("$schema").is_none());
        assert!(schema.get("additionalProperties").is_none());
        assert!(schema["properties"]["nested"].get("additionalProperties").is_none());
        assert!(schema["items"].get("additionalProperties").is_none());
        // The property definitions survive.
        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(schema["properties"]["nested"]["properties"]["deep"]["type"], "string");
    }

    #[test]
    fn a_property_actually_named_schema_is_left_alone() {
        // Describing a tool must not change what it accepts.
        let mut schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "$schema": { "type": "string" },
                "additionalProperties": { "type": "boolean" }
            }
        });

        strip_schema_keywords(&mut schema);

        assert!(schema.get("additionalProperties").is_none(), "the keyword goes");
        assert_eq!(
            schema["properties"]["$schema"]["type"], "string",
            "the property of that name stays"
        );
        assert_eq!(schema["properties"]["additionalProperties"]["type"], "boolean");
    }

    #[test]
    fn a_whole_tool_array_is_cleaned_in_one_pass() {
        let mut tools = json!([{
            "type": "function",
            "function": {
                "name": "read_file",
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": { "path": { "type": "string" } }
                }
            }
        }]);

        strip_schema_keywords(&mut tools);

        assert!(tools[0]["function"]["parameters"]
            .get("additionalProperties")
            .is_none());
        assert_eq!(tools[0]["function"]["name"], "read_file");
    }
}
