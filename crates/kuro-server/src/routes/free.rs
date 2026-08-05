//! The free-tier pool's API.
//!
//! Keys are written and never read back. The overview says which providers have
//! one stored and whether that one is currently working; the value itself only
//! ever leaves the credential store on its way into an outbound request.

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
                // A shared endpoint takes no account and no key, which changes
                // what the row should offer and what it must warn about.
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
                // A flavour with nothing behind it is shown as unavailable
                // rather than hidden, so the screen explains itself.
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

/// Store one provider's key.
///
/// Storing a key clears whatever trouble that provider was in, because the
/// commonest reason to be here is that the previous key was wrong.
pub async fn set_key(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
    Json(request): Json<SetKeyRequest>,
) -> AppResult<Json<Value>> {
    let provider = free::find(&slug)
        .ok_or_else(|| KuroError::not_found(format!("free provider `{slug}`")))?;

    // An endpoint that takes no key at all should say so rather than storing a
    // string it will never send — which would leave a key visibly "saved" and
    // changing nothing.
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

/// Allow or forbid the shared, unauthenticated endpoints.
///
/// Written to settings *and* mirrored into the pool, because the pool is
/// consulted on a path that must not read the database.
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

/// Record what the user says this provider's allowance is.
///
/// Kuro cannot discover this: the providers state it in incompatible units and
/// none of them expose it over the API. So it is asked for, kept as written,
/// and used only to draw a bar next to a number Kuro measured itself.
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

/// What each provider's allowance has actually been spent on.
///
/// Its own endpoint rather than part of [`overview`], because the overview is
/// re-read after every key edit and this is the one query whose cost grows with
/// the size of the message table.
pub async fn usage(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    // Windows are computed from the *local* clock. A day that rolled over at
    // UTC midnight is somebody else's day: for anyone west of Greenwich it
    // would file an evening's work under tomorrow.
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

    // Both bounds in UTC. `created_at` is written in UTC and the comparison is
    // a string one, so a bound carrying a local offset compares by its printed
    // digits rather than by the instant it names — which put an evening's turns
    // outside "today" for anyone whose offset crosses midnight.
    let to = chrono::Utc::now().to_rfc3339();
    let day = window(&state, &local_rfc3339(day_start), &to)?;
    let month = window(&state, &local_rfc3339(month_start), &to)?;

    // Averaged in Rust rather than in SQL so that no turns is `null` rather
    // than a division: there is no honest average over no samples.
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
                // A slug with no provider is a key removed since the turn ran.
                // Showing the slug is better than hiding the tokens it spent.
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

/// A local date-time as the RFC 3339 string `created_at` is compared against.
///
/// `created_at` is written in UTC, so a local window has to be converted rather
/// than formatted — otherwise "since local midnight" would silently mean "since
/// UTC midnight" and the day would be wrong by the offset.
fn local_rfc3339(naive: chrono::NaiveDateTime) -> String {
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&naive)
        .single()
        // A clock that jumped over this instant — the spring daylight-saving
        // gap — has no such local time. An hour's imprecision once a year beats
        // failing the screen.
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

/// Ask a provider for its model list, to find out whether the key works.
///
/// A real request rather than a shape check on the key, because the failure this
/// catches — a key that was copied from the wrong account, or has expired — looks
/// exactly like a working key until something asks.
pub async fn test_key(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let provider = free::find(&slug)
        .ok_or_else(|| KuroError::not_found(format!("free provider `{slug}`")))?;

    let stored = state.secrets.get(&free::secret_reference(provider.slug))?;

    // A shared endpoint has nothing to test a key against, so the button
    // becomes a reachability check — which is the useful question for one.
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
/// A credential store read that fails is treated as "no key", because the
/// alternative — failing the whole models list because one key could not be
/// decrypted — takes away the screen where that would be fixed.
///
/// ## Keys added under Providers count too
///
/// An OpenRouter key is an OpenRouter key. Somebody who has already added one on
/// the Providers screen has told Kuro everything it needs to use that provider's
/// free tier, and asking them to paste the same string into a second screen is
/// asking them to maintain two copies of one secret — where changing one and not
/// the other produces a failure with no visible cause.
///
/// A key pasted into the free screen still wins, because that is the more
/// specific statement: it may well be a different, throwaway account.
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

/// The key-picking half of [`stored_keys`], with the clock and the credential
/// store passed in.
///
/// Separated so the precedence rule is testable without standing up a server or
/// freezing time. `connectors` is `(provider slug, credential reference)` for
/// every enabled provider connection.
fn gather_keys(
    connectors: &[(String, String)],
    today: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> HashMap<String, String> {
    let mut keys: HashMap<String, String> = HashMap::new();

    // A trial that has lapsed is dropped here rather than filtered when
    // choosing, because "stop using this key" and "this provider is out of
    // allowance for the next few minutes" are different facts — and doing it
    // here keeps the pool free of a clock.
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

/// Read the model lists of every provider whose catalogue is missing or stale.
///
/// Concurrent and best-effort: a provider that cannot be reached keeps whatever
/// catalogue it had, and one that has never been read stays unknown — which the
/// pool treats as "try the curated models", not as "offers nothing".
///
/// Called before a free model is resolved. Once each provider's list is cached
/// this costs nothing for half an hour, and the first message after a restart
/// pays one round trip per provider rather than picking a model that was retired
/// three months ago.
/// Start a catalogue read in the background and return at once.
///
/// This is what a chat turn calls, and the reason it exists is latency that was
/// being paid in the worst possible place. `refresh_catalogues` reads every
/// stale provider concurrently, so it takes as long as the *slowest* of them —
/// up to the fifteen-second timeout — and it was awaited before a free model
/// was chosen. The cost landed on the first message after every restart and
/// again every half hour, as dead air before the first token, on the one code
/// path where the user is watching a cursor blink.
///
/// Nothing needs it to be synchronous. A cold catalogue is not a blocker: an
/// unknown provider is tried with its curated models, and a model that turns
/// out to have been retired answers 404, which is already handled as `Gone` —
/// a short cooldown and a failover to the next provider. That is one failed
/// round trip in the rare case, against fifteen seconds of stall in the common
/// one.
pub fn refresh_catalogues_in_background(state: &SharedState, keys: &HashMap<String, String>) {
    if !state.free.needs_any_catalogue(keys) {
        return;
    }
    // One read at a time. A burst of messages must not each start their own
    // sweep of the same twenty endpoints.
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

pub async fn refresh_catalogues(state: &SharedState, keys: &HashMap<String, String>) {
    let allow_keyless = state.free.allows_keyless();

    let stale: Vec<(&'static str, String, Option<String>, Quirks)> = FREE_PROVIDERS
        .iter()
        .filter(|provider| state.free.needs_catalogue(provider.slug))
        // Gating on reachability is what stops Kuro reading the catalogues of
        // shared endpoints on behalf of somebody who has switched them off.
        // Without it, merely opening the free-models screen would send requests
        // to services the user had declined to use.
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

/// Write the catalogues to storage so the next start already has them.
///
/// A cloud connector's model list has always been in the database, which is why
/// OpenRouter's four hundred models were in the picker the instant it opened. A
/// free provider's lived only in memory, so after a restart the picker rendered
/// with none of them and filled in seconds later — and the model list is fetched
/// once, not polled, so in practice it did not fill in at all. One provider
/// showed and four did not, which reads as four keys that stopped working.
fn save_catalogues(state: &SharedState) {
    let stored = state.free.catalogues();
    if let Err(error) = state
        .db
        .set_setting(KEY_CATALOGUES, &serde_json::json!(stored))
    {
        // Not worth failing a request over: the catalogues are a cache, and the
        // cost of losing them is one slow start rather than a wrong answer.
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

    /// A day well inside every provider's tier, so these tests are about key
    /// precedence and nothing else.
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
        // The complaint this fixes: an OpenRouter key already added on the
        // Providers screen, and Kuro Free behaving as though there were no key
        // at all until the same string was pasted a second time.
        let connectors = vec![("openrouter".to_string(), "provider:abc".to_string())];

        let keys = gather_keys(&connectors, TODAY, store(&[("provider:abc", "sk-or-from-providers")]));

        assert_eq!(keys.get("openrouter").map(String::as_str), Some("sk-or-from-providers"));
    }

    #[test]
    fn a_key_pasted_into_the_free_screen_wins_over_the_provider_one() {
        // The more specific statement: it may well be a second, throwaway
        // account kept deliberately apart from the paid one.
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
        // GitHub Models, Together and Chutes were removed from the table, but
        // a key pasted before that is still sitting in the credential store.
        // It must not resurrect a provider that cannot work.
        let keys = gather_keys(&[], TODAY, store(&[("free:github", "ghp_stale")]));

        assert!(keys.is_empty(), "got {keys:?}");
    }

    #[test]
    fn every_provider_in_the_table_has_a_reachable_looking_endpoint() {
        // Cheap guard against a typo in a base URL: every one must be HTTPS and
        // must not already carry the chat path, which `Quirks::chat_url`
        // appends.
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

    /// Ask every provider what it currently offers and report curated ids that
    /// have gone.
    ///
    /// Ignored, because it talks to a dozen third parties and would make CI fail
    /// whenever one of them had a bad afternoon. Run it by hand before a
    /// release — the table is hand-written and the models in it are exactly the
    /// kind of thing that rots quietly:
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
            // Only the endpoints that answer without a key can be checked
            // without a credential store; the rest need one and are skipped.
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
