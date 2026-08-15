
pub mod presets;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;

use crate::db::{CloudConnectorRecord, Db};
use crate::free::Trouble;
use crate::secrets::SecretStore;
use crate::wire::Quirks;
use crate::{KuroError, Result};

pub use presets::{Preset, PRESETS};

pub const MODEL_PREFIX: &str = "cloud:";

fn key_reference(connector_id: &str) -> String {
    format!("provider:{connector_id}")
}

const FREE_SUFFIX: &str = ":free";

pub const FREE_POOL_MODEL: &str = "kuro:free-pool";

pub fn free_models_of(models: &[String]) -> Vec<String> {
    let mut free: Vec<String> = models
        .iter()
        .filter(|model| model.ends_with(FREE_SUFFIX))
        .cloned()
        .collect();
    free.sort();
    free
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatTarget {
    Local { model_id: String },
    Remote {
        connector_id: String,
        label: String,
        base_url: String,
        authorization: Option<String>,
        model: String,
        quirks: Quirks,
        upstream: Option<String>,
    },
}

impl ChatTarget {
    pub fn upstream(&self) -> Option<&str> {
        match self {
            Self::Local { .. } => None,
            Self::Remote { upstream, .. } => upstream.as_deref(),
        }
    }

    pub fn quirks(&self) -> Quirks {
        match self {
            Self::Local { .. } => Quirks::OPENAI,
            Self::Remote { quirks, .. } => *quirks,
        }
    }
}

impl ChatTarget {
    pub fn recorded_id(&self) -> String {
        match self {
            Self::Local { model_id } => model_id.clone(),
            Self::Remote { connector_id, model, .. } => {
                format!("{MODEL_PREFIX}{connector_id}/{model}")
            }
        }
    }

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

#[derive(Debug, Clone, Serialize)]
pub struct RemoteModel {
    pub id: String,
    pub name: String,
    pub connector_id: String,
    pub connector_label: String,
    pub provider: String,
    pub pooled: bool,
    pub pool_size: usize,
    pub free: bool,
    pub specialities: Vec<&'static str>,
    pub params_b: Option<f32>,
    pub unavailable: Option<String>,
    pub fix_url: Option<String>,
}

impl RemoteModel {
    pub fn described(id: String, group: RemoteGroup<'_>, model: &str, free: bool) -> Self {
        let classified = crate::classify::classify(model);
        Self {
            id,
            name: model.to_string(),
            connector_id: group.connector_id.to_string(),
            connector_label: group.connector_label.to_string(),
            provider: group.provider.to_string(),
            pooled: false,
            pool_size: 0,
            free,
            specialities: classified
                .specialities
                .iter()
                .map(|speciality| speciality.label())
                .collect(),
            params_b: classified.params_b,
            unavailable: None,
            fix_url: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RemoteGroup<'a> {
    pub connector_id: &'a str,
    pub connector_label: &'a str,
    pub provider: &'a str,
}

#[derive(Default)]
struct PoolState {
    cursor: usize,
    troubled: HashMap<String, (Trouble, Instant)>,
}

impl PoolState {
    fn is_troubled(&mut self, model: &str) -> bool {
        let Some((trouble, since)) = self.troubled.get(model).copied() else {
            return false;
        };
        if since.elapsed() >= trouble.cooldown() {
            self.troubled.remove(model);
            return false;
        }
        true
    }
}

pub struct ProviderRegistry {
    db: Db,
    secrets: SecretStore,
    client: reqwest::Client,
    pools: Arc<Mutex<HashMap<String, PoolState>>>,
}

impl ProviderRegistry {
    pub fn new(db: Db, secrets: SecretStore, client: reqwest::Client) -> Self {
        Self {
            db,
            secrets,
            client,
            pools: Arc::new(Mutex::new(HashMap::new())),
        }
    }

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

        let _ = self.test(&record.id).await;

        self.db
            .get_cloud_connector(&record.id)?
            .ok_or_else(|| KuroError::other("the provider disappeared after being added"))
    }

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

    pub fn remote_models(&self) -> Result<Vec<RemoteModel>> {
        let mut out = Vec::new();

        for connector in self.db.list_cloud_connectors()? {
            if !connector.enabled {
                continue;
            }

            let free = free_models_of(&connector.models);
            if !free.is_empty() {
                out.push(RemoteModel {
                    id: format!("{MODEL_PREFIX}{}/{FREE_POOL_MODEL}", connector.id),
                    name: format!("{} free models", connector.label),
                    connector_id: connector.id.clone(),
                    connector_label: connector.label.clone(),
                    provider: connector.provider.clone(),
                    pooled: true,
                    pool_size: free.len(),
                    free: true,
                    specialities: Vec::new(),
                    params_b: None,
                    unavailable: None,
                    fix_url: None,
                });
            }

            for model in &connector.models {
                if !crate::classify::classify(model).kind.is_chat() {
                    continue;
                }
                out.push(RemoteModel::described(
                    format!("{MODEL_PREFIX}{}/{}", connector.id, model),
                    RemoteGroup {
                        connector_id: &connector.id,
                        connector_label: &connector.label,
                        provider: &connector.provider,
                    },
                    model,
                    free.iter().any(|entry| entry == model),
                ));
            }
        }

        Ok(out)
    }

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

        let model = if model == FREE_POOL_MODEL {
            self.choose_free(&connector)?
        } else {
            model
        };

        Ok(ChatTarget::Remote {
            authorization: Some(format!("Bearer {}", self.key_for(&connector)?)),
            upstream: Some(connector.provider.clone()),
            connector_id: connector.id,
            label: connector.label,
            base_url: connector.base_url,
            model,
            quirks: Quirks::OPENAI,
        })
    }

    fn choose_free(&self, connector: &CloudConnectorRecord) -> Result<String> {
        let free = free_models_of(&connector.models);

        if free.is_empty() {
            return Err(KuroError::bad_request(format!(
                "`{}` is not offering any free models at the moment. Refresh it under \
                 Providers, or pick a model by name.",
                connector.label
            )));
        }

        let mut pools = self.pools.lock().expect("provider pool lock");
        let state = pools.entry(connector.id.clone()).or_default();

        let start = state.cursor % free.len();

        for offset in 0..free.len() {
            let index = (start + offset) % free.len();
            if !state.is_troubled(&free[index]) {
                state.cursor = index;
                return Ok(free[index].clone());
            }
        }

        Ok(free[start].clone())
    }

    pub fn note_trouble(&self, connector_id: &str, model: &str, status: u16) {
        let Some(trouble) = Trouble::from_status(status) else {
            return;
        };

        let mut pools = self.pools.lock().expect("provider pool lock");
        let state = pools.entry(connector_id.to_string()).or_default();
        state.troubled.insert(model.to_string(), (trouble, Instant::now()));
        state.cursor = state.cursor.saturating_add(1);

        tracing::info!(connector_id, model, kind = trouble.as_str(), "free model set aside");
    }

    pub fn remove(&self, connector_id: &str) -> Result<bool> {
        let Some(connector) = self.db.get_cloud_connector(connector_id)? else {
            return Ok(false);
        };
        self.secrets.delete(&connector.keychain_ref)?;
        self.db.delete_cloud_connector(connector_id)
    }

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

async fn list_models(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>> {
    let response = client
        .get(format!("{}/models", base_url.trim_end_matches('/')))
        .bearer_auth(api_key)
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
            parse_model_id("cloud:abc-123/vendor/model-name").expect("split");

        assert_eq!(connector, "abc-123");
        assert_eq!(
            model, "vendor/model-name",
            "a provider's own name may itself contain slashes"
        );
        assert!(is_remote_model("cloud:abc-123/vendor/model-name"));
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
            authorization: Some("Bearer sk-or".to_string()),
            model: "vendor/model-name".to_string(),
            quirks: Quirks::OPENAI,
            upstream: Some("openrouter".to_string()),
        };

        let recorded = target.recorded_id();

        assert_eq!(recorded, "cloud:abc/vendor/model-name");
        assert_eq!(
            parse_model_id(&recorded).expect("split"),
            ("abc".to_string(), "vendor/model-name".to_string())
        );
        assert_eq!(target.wire_model(), "vendor/model-name", "the prefix must not be sent");
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

    fn openrouter(db: &Db) -> CloudConnectorRecord {
        let connector = db
            .insert_cloud_connector("openrouter", "OpenRouter", "https://openrouter.ai/api/v1", "r1")
            .expect("insert");
        db.set_cloud_ok(
            &connector.id,
            &[
                "vendor/model-name".to_string(),
                "deepseek/deepseek-r1:free".to_string(),
                "meta-llama/llama-3.3-70b-instruct:free".to_string(),
                "openai/gpt-4o".to_string(),
                "qwen/qwen-2.5-coder-32b-instruct:free".to_string(),
            ],
        )
        .expect("ok");
        db.get_cloud_connector(&connector.id).expect("get").expect("present")
    }

    #[test]
    fn only_ids_ending_in_free_are_pooled_and_the_order_is_stable() {
        let models = vec![
            "z/model:free".to_string(),
            "openai/gpt-4o".to_string(),
            "a/model:free".to_string(),
            "vendor/freestyle".to_string(),
            "vendor/model:free-tier".to_string(),
        ];

        assert_eq!(
            free_models_of(&models),
            vec!["a/model:free".to_string(), "z/model:free".to_string()]
        );
    }

    #[test]
    fn a_provider_with_free_models_gets_a_pool_row_at_the_top_of_its_own_list() {
        let (registry, db) = registry();
        let connector = openrouter(&db);

        let models = registry.remote_models().expect("models");
        let first = &models[0];

        assert!(first.pooled, "the pool must come before the models it pools");
        assert_eq!(first.id, format!("cloud:{}/{FREE_POOL_MODEL}", connector.id));
        assert_eq!(first.name, "OpenRouter free models");
        assert_eq!(first.pool_size, 3);
        assert_eq!(models.len(), 6, "the pool is one extra row, not a replacement");
        assert!(models[1..].iter().all(|model| !model.pooled));
    }

    #[test]
    fn a_provider_with_nothing_free_gets_no_pool_row() {
        let (registry, db) = registry();
        let connector = db
            .insert_cloud_connector("openai", "OpenAI", "https://api.openai.com/v1", "r2")
            .expect("insert");
        db.set_cloud_ok(&connector.id, &["gpt-4o".to_string()]).expect("ok");

        let models = registry.remote_models().expect("models");

        assert_eq!(models.len(), 1);
        assert!(!models[0].pooled, "an empty pool would be a row that never resolves");
    }

    #[test]
    fn the_pool_resolves_to_a_real_free_model_and_never_sends_its_own_name() {
        let (registry, db) = registry();
        let connector = openrouter(&db);
        registry.secrets.put("r1", "sk-or-test").expect("put");

        let target = registry
            .resolve_target(&format!("cloud:{}/{FREE_POOL_MODEL}", connector.id))
            .expect("resolve");

        let model = target.wire_model();
        assert!(model.ends_with(":free"), "got {model}");
        assert_ne!(model, FREE_POOL_MODEL, "the sentinel must never reach a provider");
    }

    #[test]
    fn the_pool_stays_on_one_model_until_it_refuses_then_moves_to_the_next() {
        let (registry, db) = registry();
        let connector = openrouter(&db);
        registry.secrets.put("r1", "sk-or-test").expect("put");
        let pool_id = format!("cloud:{}/{FREE_POOL_MODEL}", connector.id);

        let first = registry.resolve_target(&pool_id).expect("resolve").wire_model().to_string();
        assert_eq!(
            registry.resolve_target(&pool_id).expect("resolve").wire_model(),
            first,
            "a conversation must not change model between messages for no reason"
        );

        registry.note_trouble(&connector.id, &first, 429);

        let second = registry.resolve_target(&pool_id).expect("resolve").wire_model().to_string();
        assert_ne!(second, first, "an exhausted model is not an outage");
        assert!(second.ends_with(":free"));
    }

    #[test]
    fn every_free_model_refusing_still_produces_a_request_rather_than_an_error() {
        let (registry, db) = registry();
        let connector = openrouter(&db);
        registry.secrets.put("r1", "sk-or-test").expect("put");
        let pool_id = format!("cloud:{}/{FREE_POOL_MODEL}", connector.id);

        for model in free_models_of(&connector.models) {
            registry.note_trouble(&connector.id, &model, 429);
        }

        let target = registry.resolve_target(&pool_id).expect("resolve");
        assert!(target.wire_model().ends_with(":free"));
    }

    #[test]
    fn a_success_status_is_not_treated_as_a_refusal() {
        let (registry, db) = registry();
        let connector = openrouter(&db);
        registry.secrets.put("r1", "sk-or-test").expect("put");
        let pool_id = format!("cloud:{}/{FREE_POOL_MODEL}", connector.id);

        let first = registry.resolve_target(&pool_id).expect("resolve").wire_model().to_string();
        registry.note_trouble(&connector.id, &first, 500);

        assert_eq!(
            registry.resolve_target(&pool_id).expect("resolve").wire_model(),
            first,
            "a server error is not the model's fault"
        );
    }

    #[test]
    fn asking_a_provider_with_no_free_models_for_its_pool_says_so_plainly() {
        let (registry, db) = registry();
        let connector = db
            .insert_cloud_connector("openai", "OpenAI", "https://api.openai.com/v1", "r3")
            .expect("insert");
        db.set_cloud_ok(&connector.id, &["gpt-4o".to_string()]).expect("ok");
        registry.secrets.put("r3", "sk-test").expect("put");

        let error = registry
            .resolve_target(&format!("cloud:{}/{FREE_POOL_MODEL}", connector.id))
            .unwrap_err()
            .to_string();

        assert!(error.contains("free models"), "got: {error}");
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
