//! The free-tier pool's API.
//!
//! Keys are write-only: the overview reports whether a provider has one stored
//! and whether it works, and a key leaves the credential store only on its way
//! into an outbound request.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::Json;
use kuro_core::free::{self, FreeFlavour, Trouble, FREE_PROVIDERS};
use kuro_core::settings::{self, Allowance};
use kuro_core::wire::{Auth, Quirks};
use kuro_core::KuroError;
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// Everything the free-models screen needs in one call.
pub async fn overview(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let keys = stored_keys(&state);
    let allow_keyless = state.free.allows_keyless();
    let allowances = settings::free_allowances(&state.db).unwrap_or_default();
    let today = today();

    let providers: Vec<Value> = FREE_PROVIDERS
        .iter()
        .map(|provider| {
            let trouble = state.free.trouble(provider.slug);
            json!({
                "slug": provider.slug,
                "name": provider.name,
                "baseUrl": provider.base_url,
                "credentialsUrl": provider.credentials_url,
                "allowance": provider.allowance,
                "keyHint": provider.key_hint,
                "models": provider.models.iter().map(|model| model.id).collect::<Vec<_>>(),
                // Whether a key is stored, never the key.
                "hasKey": keys.contains_key(provider.slug),
                "trouble": trouble.map(|kind| kind.as_str()),
                "tier": provider.tier.as_str(),
                // A shared endpoint needs no account or key, which changes what the row offers.
                "keyless": provider.quirks.auth.keyless(),
                "privacy": {
                    "logged": provider.privacy.logged,
                    "trains": provider.privacy.trains,
                },
                "expired": provider.tier.expired_on(&today),
                "limit": allowances.get(provider.slug),
            })
        })
        .collect();

    let available = state.free.available(&keys);

    Ok(Json(json!({
        "allowKeyless": allow_keyless,
        "providers": providers,
        "flavours": FreeFlavour::ALL
            .iter()
            .map(|flavour| json!({
                "id": flavour.model_id(),
                "flavour": flavour.as_str(),
                "label": flavour.label(),
                "blurb": flavour.blurb(),
                // Shown as unavailable rather than hidden, so the screen explains itself.
                "available": state.free.choose(*flavour, &keys).is_some(),
            }))
            .collect::<Vec<_>>(),
        "keyCount": keys.len(),
        "availableCount": available.len(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct SetKeyRequest {
    #[serde(rename = "apiKey")]
    pub api_key: String,
}

/// Store one provider's key, clearing any trouble that provider was in (the
/// commonest reason to be here is a wrong previous key).
pub async fn set_key(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
    Json(request): Json<SetKeyRequest>,
) -> AppResult<Json<Value>> {
    let provider = free::find(&slug)
        .ok_or_else(|| KuroError::not_found(format!("free provider `{slug}`")))?;

    // A keyless endpoint should say so rather than store a string it will never
    // send, which would leave a key "saved" and changing nothing.
    if provider.quirks.auth == Auth::None {
        return Err(KuroError::bad_request(format!(
            "{} takes no key — it is a shared endpoint that anyone can reach, \
             which is also why it is used last and warns about what it logs.",
            provider.name
        ))
        .into());
    }

    let key = request.api_key.trim();
    if key.is_empty() {
        return Err(KuroError::bad_request("the key is empty").into());
    }

    state
        .secrets
        .put(&free::secret_reference(provider.slug), key)?;
    state.free.clear_trouble(provider.slug);

    Ok(Json(json!({ "slug": provider.slug, "hasKey": true })))
}

#[derive(Debug, Deserialize)]
pub struct KeylessRequest {
    pub enabled: bool,
}

/// Allow or forbid the shared, unauthenticated endpoints. Written to settings
/// and mirrored into the pool, which is read on a path that must not touch the database.
pub async fn set_keyless(
    State(state): State<SharedState>,
    Json(request): Json<KeylessRequest>,
) -> AppResult<Json<Value>> {
    state
        .db
        .set_setting(settings::KEY_FREE_ALLOW_KEYLESS, &json!(request.enabled))?;
    state.free.set_allow_keyless(request.enabled);

    Ok(Json(json!({ "allowKeyless": request.enabled })))
}

#[derive(Debug, Deserialize)]
pub struct AllowanceRequest {
    #[serde(rename = "tokensPerDay")]
    pub tokens_per_day: Option<i64>,
    #[serde(rename = "tokensPerMonth")]
    pub tokens_per_month: Option<i64>,
}

/// Record what the user says this provider's allowance is. Kuro cannot discover
/// it (providers state it in incompatible units and do not expose it), so it is
/// kept as written and used only to draw a bar next to a number Kuro measured.
pub async fn set_allowance(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
    Json(request): Json<AllowanceRequest>,
) -> AppResult<Json<Value>> {
    let provider = free::find(&slug)
        .ok_or_else(|| KuroError::not_found(format!("free provider `{slug}`")))?;

    let allowance = Allowance {
        tokens_per_day: request.tokens_per_day.filter(|value| *value > 0),
        tokens_per_month: request.tokens_per_month.filter(|value| *value > 0),
    };
    settings::set_free_allowance(&state.db, provider.slug, Some(allowance))?;

    Ok(Json(json!({ "slug": provider.slug, "limit": allowance })))
}

/// What each provider's allowance has actually been spent on. Separate from
/// [`overview`], which is re-read after every key edit, because this query's
/// cost grows with the message table.
pub async fn usage(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    // Windows use the *local* clock: a UTC-midnight day would file an evening's work
    // under tomorrow for anyone west of Greenwich.
    let now = chrono::Local::now();
    let day_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight exists");
    let month_start = now
        .date_naive()
        .with_day(1)
        .and_then(|first| first.and_hms_opt(0, 0, 0))
        .expect("the first of the month exists");

    // Both bounds in UTC: `created_at` is UTC and compared as a string, so a bound
    // with a local offset would compare by its printed digits, not the instant it names.
    let to = chrono::Utc::now().to_rfc3339();
    let day = window(&state, &local_rfc3339(day_start), &to)?;
    let month = window(&state, &local_rfc3339(month_start), &to)?;

    // Averaged here, not in SQL, so no turns gives `null` (no honest average over no samples).
    let reported_turns: i64 = month.1.turns - month.1.unreported;
    let tokens_per_turn = (reported_turns > 0).then(|| month.1.total / reported_turns);
    let days_elapsed = now.date_naive().day() as i64;

    Ok(Json(json!({
        "day": day.0,
        "month": month.0,
        "totals": { "day": day.1, "month": month.1 },
        "averages": {
            "tokensPerTurn": tokens_per_turn,
            "tokensPerDayThisMonth": (days_elapsed > 0).then(|| month.1.total / days_elapsed),
        },
    })))
}

/// One window's per-provider rows, plus the totals across them.
#[derive(Debug, Deserialize)]
pub struct ModelUsageQuery {
    /// How many days back, counting today. Defaults to 30.
    #[serde(default)]
    days: Option<u32>,
}

/// Tokens by model over the last `days` local days, for Settings → Usage. Covers
/// every model, local and remote, not only the free pool.
pub async fn usage_by_model(
    State(state): State<SharedState>,
    axum::extract::Query(query): axum::extract::Query<ModelUsageQuery>,
) -> AppResult<Json<Value>> {
    let days = query.days.unwrap_or(30).clamp(1, 366);
    let start = chrono::Local::now().date_naive() - chrono::Days::new(u64::from(days - 1));
    let from = local_rfc3339(start.and_hms_opt(0, 0, 0).expect("midnight exists"));
    let to = chrono::Utc::now().to_rfc3339();

    let models = state.db.usage_by_model(&from, &to)?;
    let sum = |pick: fn(&kuro_core::db::ModelUsage) -> i64| models.iter().map(pick).sum::<i64>();
    Ok(Json(json!({
        "days": days,
        "models": models,
        "total": {
            "turns": sum(|row| row.turns),
            "promptTokens": sum(|row| row.prompt_tokens),
            "completionTokens": sum(|row| row.completion_tokens),
            "unreportedTurns": sum(|row| row.unreported_turns),
        },
    })))
}

fn window(state: &SharedState, from: &str, to: &str) -> AppResult<(Value, Totals)> {
    let rows = state.db.usage_by_provider(from, to)?;
    let allowances = settings::free_allowances(&state.db).unwrap_or_default();

    let totals = Totals {
        turns: rows.iter().map(|row| row.turns).sum(),
        unreported: rows.iter().map(|row| row.unreported_turns).sum(),
        total: rows.iter().map(|row| row.total_tokens()).sum(),
    };

    let providers: Vec<Value> = rows
        .iter()
        .map(|row| {
            let name = free::find(&row.provider_slug).map(|held| held.name);
            json!({
                "providerSlug": row.provider_slug,
                // A slug with no provider is a key removed since the turn ran; show the slug rather
                // than hide the tokens.
                "name": name.unwrap_or(row.provider_slug.as_str()),
                "turns": row.turns,
                "promptTokens": row.prompt_tokens,
                "completionTokens": row.completion_tokens,
                "totalTokens": row.total_tokens(),
                "unreportedTurns": row.unreported_turns,
                "limit": allowances.get(row.provider_slug.as_str()),
            })
        })
        .collect();

    Ok((
        json!({
            "from": from,
            "to": to,
            "providers": providers,
            "turns": totals.turns,
            "unreportedTurns": totals.unreported,
            "totalTokens": totals.total,
        }),
        totals,
    ))
}

/// The sums across one window, kept separately so averages can use them.
#[derive(Debug, Clone, Copy, Serialize)]
struct Totals {
    turns: i64,
    unreported: i64,
    total: i64,
}

/// A local date-time as the UTC RFC 3339 string `created_at` is compared
/// against. It must be converted, not just formatted, or "since local midnight"
/// would mean "since UTC midnight".
fn local_rfc3339(naive: chrono::NaiveDateTime) -> String {
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&naive)
        .single()
        // A local time skipped by the spring DST gap has no instant; an hour of
        // imprecision once a year beats failing the screen.
        .map(|local| local.with_timezone(&chrono::Utc).to_rfc3339())
        .unwrap_or_else(|| chrono::Utc.from_utc_datetime(&naive).to_rfc3339())
}

/// Forget what the user said this provider's allowance is.
pub async fn delete_allowance(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let provider = free::find(&slug)
        .ok_or_else(|| KuroError::not_found(format!("free provider `{slug}`")))?;

    settings::set_free_allowance(&state.db, provider.slug, None)?;

    Ok(Json(json!({ "slug": provider.slug, "limit": Value::Null })))
}

pub async fn delete_key(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let provider = free::find(&slug)
        .ok_or_else(|| KuroError::not_found(format!("free provider `{slug}`")))?;

    state.secrets.delete(&free::secret_reference(provider.slug))?;
    state.free.clear_trouble(provider.slug);

    Ok(Json(json!({ "slug": provider.slug, "hasKey": false })))
}

/// Ask a provider for its model list, to find out whether the key works. A real
/// request rather than a shape check, since a key from the wrong account or an
/// expired one looks fine until something asks.
pub async fn test_key(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let provider = free::find(&slug)
        .ok_or_else(|| KuroError::not_found(format!("free provider `{slug}`")))?;

    let stored = state.secrets.get(&free::secret_reference(provider.slug))?;

    // A shared endpoint has no key to test, so the button becomes a reachability check.
    if stored.is_none() && !provider.quirks.auth.keyless() {
        return Ok(Json(json!({
            "ok": false,
            "error": "No key stored for this provider yet.",
        })));
    }

    let authorization = provider.quirks.auth.authorization(stored.as_deref());

    let mut request = state
        .outbound
        .get(format!("{}/models", provider.base_url.trim_end_matches('/')));
    if let Some(value) = &authorization {
        request = request.header(reqwest::header::AUTHORIZATION, value);
    }
    for (name, value) in provider.quirks.headers {
        request = request.header(*name, *value);
    }

    let response = request.send().await;

    match response {
        Ok(response) if response.status().is_success() => {
            state.free.clear_trouble(provider.slug);
            Ok(Json(json!({ "ok": true })))
        }
        Ok(response) => {
            let status = response.status();
            if let Some(trouble) = Trouble::from_status(status.as_u16()) {
                state.free.note_trouble(provider.slug, trouble);
            }
            let detail = response.text().await.unwrap_or_default();
            Ok(Json(json!({
                "ok": false,
                "error": format!("{} refused ({status}): {}", provider.name, first_line(&detail)),
            })))
        }
        Err(error) => Ok(Json(json!({
            "ok": false,
            "error": format!("Could not reach {}: {error}", provider.name),
        }))),
    }
}

/// Every free key Kuro can find, keyed by provider slug.
///
/// A credential store read that fails counts as "no key"; failing the whole
/// models list would remove the screen where it gets fixed.
///
/// Keys added under Providers count too (an OpenRouter key is an OpenRouter key),
/// so the user does not maintain two copies of one secret. A key pasted into the
/// free screen wins, being the more specific statement (it may be a throwaway account).
pub fn stored_keys(state: &SharedState) -> HashMap<String, String> {
    let connectors: Vec<(String, String)> = state
        .db
        .list_cloud_connectors()
        .unwrap_or_default()
        .into_iter()
        .filter(|connector| connector.enabled)
        .map(|connector| (connector.provider, connector.keychain_ref))
        .collect();

    gather_keys(&connectors, &today(), |reference| {
        state.secrets.get(reference).ok().flatten()
    })
}

/// Today, as the ISO date the tier dates are written in.
fn today() -> String {
    chrono::Utc::now().date_naive().to_string()
}

/// The key-picking half of [`stored_keys`], with the clock and credential store
/// passed in so the precedence rule is testable. `connectors` is `(provider
/// slug, credential reference)` for every enabled provider connection.
fn gather_keys(
    connectors: &[(String, String)],
    today: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> HashMap<String, String> {
    let mut keys: HashMap<String, String> = HashMap::new();

    // A lapsed trial is dropped here, not when choosing: "stop using this key"
    // differs from "out of allowance for a few minutes", and this keeps the pool free of a clock.
    let usable = |slug: &str| free::find(slug).is_some_and(|held| !held.tier.expired_on(today));

    // The connectors first, so a key added on the free screen overwrites them.
    for (provider, reference) in connectors {
        if !usable(provider) {
            continue;
        }
        if let Some(key) = lookup(reference).filter(|key| !key.trim().is_empty()) {
            keys.insert(provider.clone(), key);
        }
    }

    for provider in FREE_PROVIDERS {
        if !usable(provider.slug) {
            continue;
        }
        if let Some(key) =
            lookup(&free::secret_reference(provider.slug)).filter(|key| !key.trim().is_empty())
        {
            keys.insert(provider.slug.to_string(), key);
        }
    }

    keys
}

/// Start a catalogue read in the background and return at once.
///
/// Chat turns call this instead of awaiting `refresh_catalogues`, which takes as
/// long as the slowest provider (up to the 15 s timeout) and would put dead air
/// before the first token. Nothing needs it synchronous: an unknown provider is
/// tried with its curated models, and a retired model answers 404, which is
/// handled as `Gone` (a short cooldown and a failover).
pub fn refresh_catalogues_in_background(state: &SharedState, keys: &HashMap<String, String>) {
    if !state.free.needs_any_catalogue(keys) {
        return;
    }
    // One read at a time, so a burst of messages does not each sweep the same endpoints.
    if !state.free.begin_refresh() {
        return;
    }

    let state = state.clone();
    let keys = keys.clone();
    tokio::spawn(async move {
        refresh_catalogues(&state, &keys).await;
        state.free.end_refresh();
    });
}

/// Read the model lists of every provider whose catalogue is missing or stale.
///
/// Concurrent and best-effort: an unreachable provider keeps its old catalogue,
/// and one never read stays unknown, which the pool treats as "try the curated
/// models" rather than "offers nothing". Cached lists last half an hour.
pub async fn refresh_catalogues(state: &SharedState, keys: &HashMap<String, String>) {
    let allow_keyless = state.free.allows_keyless();

    let stale: Vec<(&'static str, String, Option<String>, Quirks)> = FREE_PROVIDERS
        .iter()
        .filter(|provider| state.free.needs_catalogue(provider.slug))
        // Only reachable providers, so a shared endpoint the user switched off is never read.
        .filter(|provider| provider.is_reachable(keys, allow_keyless))
        .map(|provider| {
            (
                provider.slug,
                provider.base_url.to_string(),
                provider
                    .quirks
                    .auth
                    .authorization(keys.get(provider.slug).map(String::as_str)),
                provider.quirks,
            )
        })
        .collect();

    if stale.is_empty() {
        return;
    }

    let reads = stale
        .into_iter()
        .map(|(slug, base_url, authorization, quirks)| async move {
            let models = list_models(state, &base_url, authorization.as_deref(), quirks).await;
            (slug, models)
        });

    let mut learned = false;
    for (slug, models) in futures::future::join_all(reads).await {
        match models {
            Some(models) => {
                tracing::debug!(slug, count = models.len(), "free catalogue read");
                state.free.set_live_models(slug, models);
                learned = true;
            }
            None => tracing::debug!(slug, "free catalogue could not be read"),
        }
    }

    if learned {
        save_catalogues(state);
    }
}

/// Settings key holding the last catalogue read of every free provider.
pub const KEY_CATALOGUES: &str = "free.catalogues";

/// Settings key holding which providers and models are currently set aside.
pub const KEY_TROUBLES: &str = "free.troubles";

/// Write the current refusals to storage, so a restart does not erase a
/// cooldown and offer a rejected key as available.
pub fn save_troubles(state: &SharedState) {
    let stored = state.free.troubles();
    if let Err(error) = state.db.set_setting(KEY_TROUBLES, &serde_json::json!(stored)) {
        tracing::warn!(%error, "could not store free provider cooldowns");
    }
}

/// Read back the refusals from the last run, dropping any that have expired.
pub fn stored_troubles(db: &kuro_core::db::Db) -> Vec<(String, String, u64)> {
    db.get_setting(KEY_TROUBLES)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// Write the catalogues to storage so the next start already has them.
/// Otherwise the picker starts without a free provider's models and, since the
/// list is fetched once rather than polled, stays that way.
fn save_catalogues(state: &SharedState) {
    let stored = state.free.catalogues();
    if let Err(error) = state
        .db
        .set_setting(KEY_CATALOGUES, &serde_json::json!(stored))
    {
        // A cache: losing it costs one slow start, not a wrong answer, so don't fail the request.
        tracing::warn!(%error, "could not store free catalogues");
    }
}

/// Read back what the last run learned.
pub fn stored_catalogues(db: &kuro_core::db::Db) -> HashMap<String, Vec<String>> {
    db.get_setting(KEY_CATALOGUES)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// One provider's advertised model ids.
async fn list_models(
    state: &SharedState,
    base_url: &str,
    authorization: Option<&str>,
    quirks: Quirks,
) -> Option<Vec<String>> {
    let mut request = state
        .outbound
        .get(format!("{}/models", base_url.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(15));

    if let Some(value) = authorization {
        request = request.header(reqwest::header::AUTHORIZATION, value);
    }
    for (name, value) in quirks.headers {
        request = request.header(*name, *value);
    }

    let response = request.send().await.ok()?;

    if !response.status().is_success() {
        return None;
    }

    let body: Value = response.json().await.ok()?;
    Some(
        body.get("data")?
            .as_array()?
            .iter()
            .filter_map(|entry| entry.get("id")?.as_str().map(str::to_string))
            .collect(),
    )
}

/// A provider's error body is often a page of JSON; one line of it is enough.
fn first_line(text: &str) -> String {
    const LIMIT: usize = 200;
    let trimmed = text.trim();
    let line = trimmed.lines().next().unwrap_or(trimmed);
    if line.chars().count() <= LIMIT {
        return line.to_string();
    }
    format!("{}…", line.chars().take(LIMIT).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_body_is_shortened_to_something_a_person_reads() {
        assert_eq!(first_line("  bad key  "), "bad key");
        assert_eq!(first_line("first\nsecond"), "first");

        let long = first_line(&"x".repeat(500));
        assert!(long.chars().count() <= 201);
        assert!(long.ends_with('…'));
    }

    /// A day inside every provider's tier, so these tests exercise only key precedence.
    const TODAY: &str = "2026-07-31";

    /// A credential store as a list of pairs.
    fn store<'a>(entries: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |wanted| {
            entries
                .iter()
                .find(|(reference, _)| *reference == wanted)
                .map(|(_, key)| (*key).to_string())
        }
    }

    #[test]
    fn a_key_added_under_providers_is_enough_for_the_free_pool() {
        // An OpenRouter key added under Providers should be enough, with no second paste.
        let connectors = vec![("openrouter".to_string(), "provider:abc".to_string())];

        let keys = gather_keys(&connectors, TODAY, store(&[("provider:abc", "sk-or-from-providers")]));

        assert_eq!(keys.get("openrouter").map(String::as_str), Some("sk-or-from-providers"));
    }

    #[test]
    fn a_key_pasted_into_the_free_screen_wins_over_the_provider_one() {
        // The more specific statement: it may be a deliberately separate throwaway account.
        let connectors = vec![("openrouter".to_string(), "provider:abc".to_string())];

        let keys = gather_keys(
            &connectors,
            TODAY,
            store(&[("provider:abc", "sk-or-paid"), ("free:openrouter", "sk-or-free")]),
        );

        assert_eq!(keys.get("openrouter").map(String::as_str), Some("sk-or-free"));
    }

    #[test]
    fn a_connector_for_something_the_free_pool_does_not_know_is_ignored() {
        // Anthropic has no free tier, so an Anthropic key must not become one.
        let connectors = vec![
            ("anthropic".to_string(), "provider:one".to_string()),
            ("custom".to_string(), "provider:two".to_string()),
        ];

        let keys = gather_keys(
            &connectors,
            TODAY,
            store(&[("provider:one", "sk-ant"), ("provider:two", "sk-box")]),
        );

        assert!(keys.is_empty(), "got {keys:?}");
    }

    #[test]
    fn a_blank_stored_key_is_not_a_key() {
        let connectors = vec![("groq".to_string(), "provider:abc".to_string())];
        assert!(gather_keys(&connectors, TODAY, store(&[("provider:abc", "   ")])).is_empty());
    }

    #[test]
    fn no_connectors_and_no_pasted_keys_is_simply_empty() {
        assert!(gather_keys(&[], TODAY, store(&[])).is_empty());
    }

    #[test]
    fn a_key_for_a_provider_kuro_has_dropped_is_ignored() {
        // GitHub Models, Together and Chutes were dropped from the table, but a key
        // pasted earlier may remain in the store; it must not resurrect them.
        let keys = gather_keys(&[], TODAY, store(&[("free:github", "ghp_stale")]));

        assert!(keys.is_empty(), "got {keys:?}");
    }

    #[test]
    fn every_provider_in_the_table_has_a_reachable_looking_endpoint() {
        // Guards against a base-URL typo: HTTPS, and no chat path (`Quirks::chat_url` appends it).
        for provider in FREE_PROVIDERS {
            assert!(provider.base_url.starts_with("https://"), "{}", provider.slug);
            assert!(
                !provider.base_url.contains("/chat/completions"),
                "`{}` carries an endpoint where a base URL belongs",
                provider.slug
            );
            assert!(
                provider.quirks.chat_url(provider.base_url).ends_with(provider.quirks.chat_path),
                "{}",
                provider.slug
            );
        }
    }

    /// Asks every provider what it offers and reports curated ids that have gone.
    ///
    /// Ignored because it talks to a dozen third parties and would fail CI whenever
    /// one has a bad afternoon. Run it by hand before a release; the hand-written
    /// table rots quietly:
    ///
    /// ```text
    /// cargo test -p kuro-server curated_ids_still_exist -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "talks to every provider; run by hand before a release"]
    async fn curated_ids_still_exist() {
        let client = reqwest::Client::new();
        let mut stale: Vec<String> = Vec::new();

        for provider in FREE_PROVIDERS {
            // Only keyless endpoints can be checked without a credential store.
            if !provider.quirks.auth.keyless() {
                continue;
            }

            let url = format!("{}/models", provider.base_url.trim_end_matches('/'));
            let Ok(response) = client.get(&url).send().await else {
                println!("  {}: unreachable", provider.slug);
                continue;
            };
            let Ok(body) = response.json::<Value>().await else {
                println!("  {}: unreadable catalogue", provider.slug);
                continue;
            };
            let Some(ids) = body.get("data").and_then(Value::as_array) else {
                println!("  {}: no `data` array", provider.slug);
                continue;
            };
            let live: Vec<&str> = ids.iter().filter_map(|m| m.get("id")?.as_str()).collect();

            for model in provider.models {
                if !live.contains(&model.id) {
                    stale.push(format!("{}: {}", provider.slug, model.id));
                }
            }
            println!("  {}: {} models advertised", provider.slug, live.len());
        }

        assert!(stale.is_empty(), "curated ids no longer offered:\n  {}", stale.join("\n  "));
    }
}
