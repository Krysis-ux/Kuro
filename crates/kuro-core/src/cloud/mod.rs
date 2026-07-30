//! Remote model providers.
//!
//! Kuro runs models locally. This module is the escape hatch for when the machine
//! cannot: an endpoint you hold the key for, whose models appear in the same
//! picker as the local ones. OpenRouter, OpenAI, Anthropic, Groq, or the
//! OpenAI-compatible URL of a GPU box you rented for the afternoon.
//!
//! Three decisions keep this from turning Kuro into a generic API client:
//!
//! * **One wire format.** Everything is spoken to as the OpenAI API, which is why
//!   Anthropic is reached through its compatibility endpoint rather than through a
//!   second code path. A provider that cannot do that is not supported, which is
//!   better than half-supporting it.
//! * **The user's account, always.** There is no Kuro-hosted anything here. The
//!   key is theirs, the bill is theirs, and the request goes straight to the
//!   provider.
//! * **Never the quiet default.** A remote model is always visibly remote, and
//!   local stays first in the list. The promise on the front page is that nothing
//!   leaves the machine unless you ask.

pub mod presets;

use serde::Serialize;

use crate::db::{CloudConnectorRecord, Db};
use crate::secrets::SecretStore;
use crate::{KuroError, Result};

pub use presets::{Preset, PRESETS};

/// Prefix on a model id that marks it as belonging to a provider.
///
/// `cloud:<connector id>/<model name>`. A prefix rather than a separate field
/// because it means a provider model can travel anywhere a local model id can —
/// the composer, a conversation row, the OpenAI-compatible API — without every
/// one of those needing to learn about providers.
pub const MODEL_PREFIX: &str = "cloud:";

/// Credential reference for a provider. Namespaced away from MCP tokens.
fn key_reference(connector_id: &str) -> String {
    format!("provider:{connector_id}")
}

/// Where a chat request should be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatTarget {
    /// A local engine process, addressed by model id.
    Local { model_id: String },
    /// A provider endpoint.
    Remote {
        connector_id: String,
        label: String,
        base_url: String,
        api_key: String,
        /// The name the provider knows the model by, without Kuro's prefix.
        model: String,
    },
}

impl ChatTarget {
    /// The model id as Kuro records it, which is what goes in the database.
    pub fn recorded_id(&self) -> String {
        match self {
            Self::Local { model_id } => model_id.clone(),
            Self::Remote { connector_id, model, .. } => {
                format!("{MODEL_PREFIX}{connector_id}/{model}")
            }
        }
    }

    /// The name to put in the request body.
    pub fn wire_model(&self) -> &str {
        match self {
            Self::Local { model_id } => model_id,
            Self::Remote { model, .. } => model,
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }
}

/// Split `cloud:<id>/<model>` into its parts.
///
/// Returns `None` for a plain local model id, which is how callers tell the two
/// apart without a separate flag.
pub fn parse_model_id(model_id: &str) -> Option<(String, String)> {
    let rest = model_id.strip_prefix(MODEL_PREFIX)?;
    let (connector_id, model) = rest.split_once('/')?;

    if connector_id.is_empty() || model.is_empty() {
        return None;
    }
    Some((connector_id.to_string(), model.to_string()))
}

pub fn is_remote_model(model_id: &str) -> bool {
    parse_model_id(model_id).is_some()
}

/// A provider's models, for the picker.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteModel {
    /// The prefixed id, usable anywhere a model id is.
    pub id: String,
    /// The provider's own name for it.
    pub name: String,
    pub connector_id: String,
    pub connector_label: String,
    pub provider: String,
}

pub struct ProviderRegistry {
    db: Db,
    secrets: SecretStore,
    client: reqwest::Client,
}

impl ProviderRegistry {
    pub fn new(db: Db, secrets: SecretStore, client: reqwest::Client) -> Self {
        Self { db, secrets, client }
    }

    /// Add a provider: store the key, record the endpoint, then probe it.
    ///
    /// The probe happens immediately because a key that does not work is worth
    /// knowing about while the user is still looking at the form, not the first
    /// time they try to send a message.
    pub async fn add(
        &self,
        provider: &str,
        label: Option<&str>,
        base_url: Option<&str>,
        api_key: &str,
    ) -> Result<CloudConnectorRecord> {
        let preset = presets::find(provider);

        let resolved_url = base_url
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_string)
            // A preset that asks for a URL carries an empty one, which must not
            // be mistaken for a default.
            .or_else(|| {
                preset
                    .map(|preset| preset.base_url)
                    .filter(|url| !url.is_empty())
                    .map(str::to_string)
            })
            .ok_or_else(|| {
                KuroError::bad_request(
                    "a custom provider needs the base URL of its OpenAI-compatible API",
                )
            })?;

        if !resolved_url.starts_with("http://") && !resolved_url.starts_with("https://") {
            return Err(KuroError::bad_request(format!(
                "`{resolved_url}` is not an http or https URL"
            )));
        }

        let resolved_label = label
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .or_else(|| preset.map(|preset| preset.name.to_string()))
            .ok_or_else(|| KuroError::bad_request("the provider needs a name"))?;

        // The row is created first so the credential reference has an id to hang
        // off, then the key is written. A failure between the two leaves a
        // provider with no key, which the interface shows as "needs a key" — a
        // recoverable state, unlike an orphaned secret.
        let record = self.db.insert_cloud_connector(
            provider,
            &resolved_label,
            &resolved_url,
            "pending",
        )?;

        let reference = key_reference(&record.id);
        if let Err(error) = self.secrets.put(&reference, api_key) {
            let _ = self.db.delete_cloud_connector(&record.id);
            return Err(error);
        }

        self.db.with(|conn| {
            conn.execute(
                "UPDATE cloud_connectors SET keychain_ref = ?2 WHERE id = ?1",
                rusqlite::params![record.id, reference],
            )?;
            Ok(())
        })?;

        // A failed probe is recorded, not returned: the provider is saved and the
        // user can fix the key without re-entering everything.
        let _ = self.test(&record.id).await;

        self.db
            .get_cloud_connector(&record.id)?
            .ok_or_else(|| KuroError::other("the provider disappeared after being added"))
    }

    /// Ask a provider what models it offers, and record the answer.
    pub async fn test(&self, connector_id: &str) -> Result<Vec<String>> {
        let connector = self.require(connector_id)?;
        let key = self.key_for(&connector)?;

        match list_models(&self.client, &connector.base_url, &key).await {
            Ok(models) => {
                self.db.set_cloud_ok(&connector.id, &models)?;
                Ok(models)
            }
            Err(error) => {
                self.db.set_cloud_error(&connector.id, &error.to_string())?;
                Err(error)
            }
        }
    }

    /// Every model from every enabled, working provider.
    pub fn remote_models(&self) -> Result<Vec<RemoteModel>> {
        let mut out = Vec::new();

        for connector in self.db.list_cloud_connectors()? {
            if !connector.enabled {
                continue;
            }
            for model in &connector.models {
                out.push(RemoteModel {
                    id: format!("{MODEL_PREFIX}{}/{}", connector.id, model),
                    name: model.clone(),
                    connector_id: connector.id.clone(),
                    connector_label: connector.label.clone(),
                    provider: connector.provider.clone(),
                });
            }
        }

        Ok(out)
    }

    /// Resolve a prefixed model id into somewhere to send a request.
    pub fn resolve_target(&self, model_id: &str) -> Result<ChatTarget> {
        let Some((connector_id, model)) = parse_model_id(model_id) else {
            return Ok(ChatTarget::Local {
                model_id: model_id.to_string(),
            });
        };

        let connector = self.require(&connector_id)?;
        if !connector.enabled {
            return Err(KuroError::bad_request(format!(
                "`{}` is switched off",
                connector.label
            )));
        }

        Ok(ChatTarget::Remote {
            api_key: self.key_for(&connector)?,
            connector_id: connector.id,
            label: connector.label,
            base_url: connector.base_url,
            model,
        })
    }

    /// Remove a provider and its key together.
    pub fn remove(&self, connector_id: &str) -> Result<bool> {
        let Some(connector) = self.db.get_cloud_connector(connector_id)? else {
            return Ok(false);
        };
        // The key goes first: a leftover secret is worse than a leftover row,
        // which the user can see and delete again.
        self.secrets.delete(&connector.keychain_ref)?;
        self.db.delete_cloud_connector(connector_id)
    }

    /// Replace a provider's key in place, keeping its id and conversations.
    pub async fn replace_key(&self, connector_id: &str, api_key: &str) -> Result<()> {
        let connector = self.require(connector_id)?;
        self.secrets.put(&connector.keychain_ref, api_key)?;
        let _ = self.test(connector_id).await;
        Ok(())
    }

    pub fn has_key(&self, connector: &CloudConnectorRecord) -> bool {
        self.secrets.has(&connector.keychain_ref)
    }

    fn require(&self, connector_id: &str) -> Result<CloudConnectorRecord> {
        self.db
            .get_cloud_connector(connector_id)?
            .ok_or_else(|| KuroError::not_found(format!("provider `{connector_id}`")))
    }

    fn key_for(&self, connector: &CloudConnectorRecord) -> Result<String> {
        self.secrets
            .get(&connector.keychain_ref)?
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| {
                KuroError::bad_request(format!(
                    "`{}` has no API key stored. Add one in Providers.",
                    connector.label
                ))
            })
    }
}

/// Ask an OpenAI-compatible endpoint for its model list.
async fn list_models(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>> {
    let response = client
        .get(format!("{}/models", base_url.trim_end_matches('/')))
        .bearer_auth(api_key)
        // Anthropic's compatibility endpoint requires its own version header even
        // when the OpenAI shape is being used.
        .header("anthropic-version", "2023-06-01")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|error| {
            if error.is_connect() || error.is_timeout() {
                KuroError::other(format!("could not reach {base_url}. Check the URL."))
            } else {
                KuroError::other(format!("{error}"))
            }
        })?;

    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        return Err(describe_failure(status, &body));
    }

    let parsed: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
        KuroError::other(format!("that endpoint did not answer with JSON: {error}"))
    })?;

    let mut models: Vec<String> = parsed
        .get("data")
        .and_then(|data| data.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("id").and_then(|id| id.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    if models.is_empty() {
        return Err(KuroError::other(
            "that endpoint answered but listed no models. \
             It may not be an OpenAI-compatible API.",
        ));
    }

    models.sort();
    models.dedup();
    Ok(models)
}

fn describe_failure(status: reqwest::StatusCode, body: &str) -> KuroError {
    let detail: String = body.trim().chars().take(200).collect();

    match status.as_u16() {
        401 | 403 => KuroError::bad_request(format!(
            "the provider rejected the key ({status}). Check it was pasted in full."
        )),
        404 => KuroError::bad_request(format!(
            "nothing at that URL ({status}). \
             The base URL should be the part before `/chat/completions`, usually ending in `/v1`."
        )),
        429 => KuroError::other(format!("the provider rate-limited the request ({status}).")),
        _ => KuroError::other(format!("the provider returned {status}: {detail}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> (ProviderRegistry, Db) {
        let db = Db::open_in_memory().expect("open");
        let path = std::env::temp_dir().join(format!("kuro-prov-{}.json", uuid::Uuid::new_v4()));
        let registry = ProviderRegistry::new(db.clone(), SecretStore::new(path), reqwest::Client::new());
        (registry, db)
    }

    #[test]
    fn a_plain_model_id_is_local() {
        assert_eq!(parse_model_id("qwen3-4b:q4_k_m"), None);
        assert!(!is_remote_model("qwen3-4b:q4_k_m"));
    }

    #[test]
    fn a_prefixed_model_id_splits_into_connector_and_model() {
        let (connector, model) =
            parse_model_id("cloud:abc-123/anthropic/claude-opus-5").expect("split");

        assert_eq!(connector, "abc-123");
        assert_eq!(
            model, "anthropic/claude-opus-5",
            "a provider's own name may itself contain slashes"
        );
        assert!(is_remote_model("cloud:abc-123/anthropic/claude-opus-5"));
    }

    #[test]
    fn a_malformed_prefix_is_not_treated_as_remote() {
        assert_eq!(parse_model_id("cloud:"), None);
        assert_eq!(parse_model_id("cloud:abc"), None);
        assert_eq!(parse_model_id("cloud:/model"), None);
        assert_eq!(parse_model_id("cloud:abc/"), None);
    }

    #[test]
    fn a_target_round_trips_through_its_recorded_id() {
        let target = ChatTarget::Remote {
            connector_id: "abc".to_string(),
            label: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: "sk-or".to_string(),
            model: "anthropic/claude-opus-5".to_string(),
        };

        let recorded = target.recorded_id();

        assert_eq!(recorded, "cloud:abc/anthropic/claude-opus-5");
        assert_eq!(
            parse_model_id(&recorded).expect("split"),
            ("abc".to_string(), "anthropic/claude-opus-5".to_string())
        );
        assert_eq!(target.wire_model(), "anthropic/claude-opus-5", "the prefix must not be sent");
        assert!(target.is_remote());
    }

    #[test]
    fn a_local_target_is_its_own_id() {
        let target = ChatTarget::Local {
            model_id: "qwen3-4b:q4_k_m".to_string(),
        };
        assert_eq!(target.recorded_id(), "qwen3-4b:q4_k_m");
        assert_eq!(target.wire_model(), "qwen3-4b:q4_k_m");
        assert!(!target.is_remote());
    }

    #[test]
    fn resolving_an_unprefixed_id_never_touches_the_provider_table() {
        let (registry, _db) = registry();
        let target = registry.resolve_target("qwen3-4b").expect("resolve");
        assert_eq!(target, ChatTarget::Local { model_id: "qwen3-4b".to_string() });
    }

    #[test]
    fn resolving_an_unknown_connector_is_a_not_found() {
        let (registry, _db) = registry();
        let error = registry.resolve_target("cloud:missing/gpt-4o").unwrap_err();
        assert!(matches!(error, KuroError::NotFound(_)));
    }

    #[test]
    fn a_provider_with_no_stored_key_says_so_rather_than_sending_an_empty_one() {
        let (registry, db) = registry();
        let connector = db
            .insert_cloud_connector("custom", "Box", "https://box.example/v1", "provider:none")
            .expect("insert");
        db.set_cloud_ok(&connector.id, &["local-model".to_string()]).expect("ok");

        let error = registry
            .resolve_target(&format!("cloud:{}/local-model", connector.id))
            .unwrap_err()
            .to_string();

        assert!(error.contains("no API key"), "got: {error}");
        assert!(!registry.has_key(&connector));
    }

    #[test]
    fn a_disabled_provider_refuses_to_resolve() {
        let (registry, db) = registry();
        let connector = db
            .insert_cloud_connector("custom", "Box", "https://box.example/v1", "provider:x")
            .expect("insert");
        db.set_cloud_enabled(&connector.id, false).expect("disable");

        let error = registry
            .resolve_target(&format!("cloud:{}/m", connector.id))
            .unwrap_err()
            .to_string();

        assert!(error.contains("switched off"), "got: {error}");
    }

    #[test]
    fn remote_models_are_listed_with_prefixed_ids_and_skip_disabled_providers() {
        let (registry, db) = registry();

        let live = db
            .insert_cloud_connector("openrouter", "OpenRouter", "https://openrouter.ai/api/v1", "r1")
            .expect("insert");
        db.set_cloud_ok(&live.id, &["a/model".to_string(), "b/model".to_string()])
            .expect("ok");

        let off = db
            .insert_cloud_connector("openai", "OpenAI", "https://api.openai.com/v1", "r2")
            .expect("insert");
        db.set_cloud_ok(&off.id, &["gpt-4o".to_string()]).expect("ok");
        db.set_cloud_enabled(&off.id, false).expect("disable");

        let models = registry.remote_models().expect("models");

        assert_eq!(models.len(), 2);
        assert!(models.iter().all(|model| model.id.starts_with(MODEL_PREFIX)));
        assert!(models.iter().all(|model| model.connector_label == "OpenRouter"));
        assert!(!models.iter().any(|model| model.name == "gpt-4o"));
    }

    #[tokio::test]
    async fn a_custom_provider_without_a_url_is_rejected() {
        let (registry, _db) = registry();
        let error = registry.add("custom", Some("Box"), None, "key").await.unwrap_err().to_string();
        assert!(error.contains("base URL"), "got: {error}");
    }

    #[tokio::test]
    async fn a_non_http_url_is_rejected() {
        let (registry, _db) = registry();
        let error = registry
            .add("custom", Some("Box"), Some("box.example/v1"), "key")
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("not an http"), "got: {error}");
    }

    #[tokio::test]
    async fn removing_a_provider_takes_its_key_with_it() {
        let (registry, db) = registry();
        let connector = db
            .insert_cloud_connector("custom", "Box", "https://box.example/v1", "provider:test")
            .expect("insert");
        registry.secrets.put("provider:test", "secret").expect("put");

        assert!(registry.has_key(&connector));
        assert!(registry.remove(&connector.id).expect("remove"));

        assert!(!registry.secrets.has("provider:test"), "the key must not outlive the provider");
        assert!(!registry.remove(&connector.id).expect("second remove"));
    }

    #[test]
    fn credential_references_are_namespaced_away_from_mcp_tokens() {
        assert_eq!(key_reference("abc"), "provider:abc");
    }

    #[test]
    fn endpoint_failures_are_translated_into_advice() {
        let unauthorized = describe_failure(reqwest::StatusCode::UNAUTHORIZED, "").to_string();
        assert!(unauthorized.contains("rejected the key"), "got: {unauthorized}");

        let missing = describe_failure(reqwest::StatusCode::NOT_FOUND, "").to_string();
        assert!(missing.contains("/v1"), "the message should say what a base URL looks like");

        let other = describe_failure(reqwest::StatusCode::BAD_GATEWAY, "gateway down");
        assert!(other.to_string().contains("gateway down"));
    }
}
