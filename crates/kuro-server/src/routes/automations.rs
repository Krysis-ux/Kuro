//! Automations: saved prompts that run on a schedule, each reply landing in a chat the user can
//! open.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::Json;
use kuro_core::db::{AutomationInput, AutomationRecord};
use kuro_core::KuroError;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::time::MissedTickBehavior;

use crate::error::AppResult;
use crate::routes::chat::{self, SendMessageRequest};
use crate::state::SharedState;

/// How often the scheduler looks for due automations. Schedules are in whole
/// minutes, so a run starts at most this late.
const TICK: Duration = Duration::from_secs(30);

/// Automations with a run in progress, so a second run (Run now pressed twice, or
/// a schedule shorter than a reply) does not interleave with it in the same chat.
// ponytail: process-wide set; fine while one daemon owns the database.
static RUNNING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// An automation as the API shows it: what is stored, and whether a run is under way.
#[derive(Serialize)]
pub struct AutomationView {
    #[serde(flatten)]
    automation: AutomationRecord,
    running: bool,
}

impl From<AutomationRecord> for AutomationView {
    fn from(automation: AutomationRecord) -> Self {
        let running = is_running(&automation.id);
        Self { automation, running }
    }
}

pub async fn list_automations(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let automations: Vec<AutomationView> =
        state.db.list_automations()?.into_iter().map(AutomationView::from).collect();
    Ok(Json(json!({ "automations": automations })))
}

pub async fn create_automation(
    State(state): State<SharedState>,
    Json(input): Json<AutomationInput>,
) -> AppResult<Json<AutomationView>> {
    Ok(Json(state.db.create_automation(&input)?.into()))
}

pub async fn update_automation(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(input): Json<AutomationInput>,
) -> AppResult<Json<AutomationView>> {
    let updated = state
        .db
        .update_automation(&id, &input)?
        .ok_or_else(|| KuroError::not_found(format!("automation `{id}`")))?;
    Ok(Json(updated.into()))
}

/// Remove an automation. The chat its runs went to stays.
pub async fn delete_automation(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    if !state.db.delete_automation(&id)? {
        return Err(KuroError::not_found(format!("automation `{id}`")).into());
    }
    Ok(Json(json!({ "deleted": true })))
}

/// Run one now, whatever its schedule. Answers once the prompt is in the chat;
/// the reply follows there.
pub async fn run_automation(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<AutomationView>> {
    let automation = state
        .db
        .get_automation(&id)?
        .ok_or_else(|| KuroError::not_found(format!("automation `{id}`")))?;
    if is_running(&id) {
        return Err(KuroError::bad_request(format!("“{}” is already running.", automation.name)).into());
    }
    Ok(Json(start_run(&state, &automation).await?.into()))
}

/// Wake every [`TICK`] and start whatever is due.
pub async fn run_scheduler(state: SharedState) {
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        let due = match state.db.due_automations(chrono::Utc::now()) {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "could not read automations");
                continue;
            }
        };
        // One still running from its last slot runs again once it finishes, not alongside.
        for automation in due.iter().filter(|automation| !is_running(&automation.id)) {
            match start_run(&state, automation).await {
                Ok(_) => tracing::info!(automation = %automation.name, "automation started"),
                Err(error) => tracing::warn!(automation = %automation.name, %error, "automation could not start"),
            }
        }
    }
}

/// Send the prompt into the automation's chat and let the reply finish in the
/// background. Returns the automation as it stands once the prompt is in.
async fn start_run(state: &SharedState, automation: &AutomationRecord) -> Result<AutomationRecord, KuroError> {
    if !claim(&automation.id) {
        return Err(KuroError::bad_request(format!("“{}” is already running.", automation.name)));
    }
    let started = begin(state, automation).await;
    if started.is_err() {
        release(&automation.id);
    }
    started?;
    state
        .db
        .get_automation(&automation.id)?
        .ok_or_else(|| KuroError::not_found(format!("automation `{}`", automation.id)))
}

async fn begin(state: &SharedState, automation: &AutomationRecord) -> Result<(), KuroError> {
    let conversation_id = conversation_for(state, automation)?;
    // Recorded before the turn so the scheduler does not start it again while a slow
    // reply streams, and a failure waits for the next slot rather than the next tick.
    state
        .db
        .record_automation_run(&automation.id, &conversation_id, chrono::Local::now())?;

    let request = SendMessageRequest {
        content: automation.prompt.clone(),
        // `default` rather than nothing: nothing would reuse the model the chat last ran on.
        model: Some(automation.model.clone().unwrap_or_else(|| "default".to_string())),
        effort: None,
        tools: None,
        web_search: None,
        skills: None,
    };
    let (mut events, turn) = match chat::start_turn(state, &conversation_id, request).await {
        Ok(started) => started,
        Err(error) => {
            state.db.set_automation_error(&automation.id, Some(&error.to_string()))?;
            return Err(error);
        }
    };

    let state = state.clone();
    let id = automation.id.clone();
    tokio::spawn(async move {
        // Nobody is listening, but the turn stops if its stream is dropped.
        while events.recv().await.is_some() {}
        let error = match turn.await {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(error) => Some(format!("the run stopped unexpectedly: {error}")),
        };
        if let Err(stored) = state.db.set_automation_error(&id, error.as_deref()) {
            tracing::warn!(error = %stored, "could not record how an automation ended");
        }
        release(&id);
    });
    Ok(())
}

/// The chat runs go to: the one from earlier runs, or a new one named after the
/// automation if there is none yet or the user deleted it.
fn conversation_for(state: &SharedState, automation: &AutomationRecord) -> Result<String, KuroError> {
    if let Some(id) = &automation.conversation_id {
        if state.db.get_conversation(id)?.is_some() {
            return Ok(id.clone());
        }
    }
    let conversation = state.db.create_conversation(None)?;
    state.db.set_conversation_title(&conversation.id, &automation.name, true)?;
    Ok(conversation.id)
}

fn running() -> std::sync::MutexGuard<'static, BTreeSet<String>> {
    // A panic elsewhere while holding it leaves the set itself intact.
    RUNNING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn is_running(id: &str) -> bool {
    running().contains(id)
}

/// Mark one as running; false if it already was.
fn claim(id: &str) -> bool {
    running().insert(id.to_string())
}

fn release(id: &str) {
    running().remove(id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_automation_cannot_be_claimed_twice_until_released() {
        assert!(claim("a1"));
        assert!(!claim("a1"));
        assert!(is_running("a1"));
        release("a1");
        assert!(!is_running("a1"));
        assert!(claim("a1"));
        release("a1");
    }
}
