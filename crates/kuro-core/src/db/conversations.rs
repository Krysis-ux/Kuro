use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;
use serde_json::Value;

use super::{now, Db};
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub title_mode: String,
    pub model_id: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
    /// The conversation this one was branched from, when it was.
    pub forked_from_id: Option<String>,
    /// The coding workspace this conversation belongs to, when it is one of the
    /// Code page's rather than an ordinary chat. This is the only thing that
    /// gives a turn access to files.
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub attachments: Option<Value>,
    pub used_web_search: bool,
    pub web_sources: Option<Value>,
    pub model_id: Option<String>,
    pub usage_prompt_tokens: Option<i64>,
    pub usage_completion_tokens: Option<i64>,
    pub timing_ttft_ms: Option<i64>,
    pub timing_total_ms: Option<i64>,
    pub timing_tokens_per_sec: Option<f64>,
    pub finish_reason: Option<String>,
    pub created_at: String,
}

/// A message to append. Only `role` and `content` are normally set; the rest
/// describe assistant turns that used tools or web search.
#[derive(Debug, Clone, Default)]
pub struct NewMessage {
    pub role: String,
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub attachments: Option<Value>,
    pub used_web_search: bool,
    pub web_sources: Option<Value>,
    pub model_id: Option<String>,
    /// Which provider's allowance this turn spends.
    ///
    /// Set at insert time, because the provider is chosen before the row is
    /// created and a turn cannot change provider once it has started — a
    /// provider that refuses is set aside for the *next* message rather than
    /// swapped mid-reply. One row is therefore always exactly one provider,
    /// which is the invariant the usage totals rest on.
    pub provider_slug: Option<String>,
}

impl NewMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            ..Default::default()
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            ..Default::default()
        }
    }
}

/// Usage and timing recorded once a streamed assistant turn finishes.
#[derive(Debug, Clone, Default)]
pub struct MessageCompletion {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub usage_prompt_tokens: Option<i64>,
    /// Prompt tokens across every round, where the field above holds the last
    /// round's only.
    ///
    /// Both are wanted, by different readers. The inspector shows what was
    /// processed to produce the answer, which is the last round. An allowance
    /// paid for all of them: a five-round agentic turn sent five prompts and
    /// was charged for five, and recording one understated the spend by most of
    /// it.
    pub usage_prompt_tokens_total: Option<i64>,
    pub usage_completion_tokens: Option<i64>,
    pub timing_ttft_ms: Option<i64>,
    pub timing_total_ms: Option<i64>,
    pub timing_tokens_per_sec: Option<f64>,
    pub finish_reason: Option<String>,
    /// Tool calls the turn made, in order, as a JSON array. Stored on the
    /// assistant row rather than as separate messages so that reloading a
    /// conversation shows the same tool trail the user watched appear.
    pub tool_calls: Option<Value>,
    pub used_web_search: bool,
    /// Pages the turn drew on, as a JSON array of `{title, url}`.
    pub web_sources: Option<Value>,
}

pub(super) fn conversation_from_row(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get("id")?,
        title: row.get("title")?,
        title_mode: row.get("title_mode")?,
        model_id: row.get("model_id")?,
        pinned: row.get::<_, i64>("pinned")? != 0,
        archived: row.get::<_, i64>("archived")? != 0,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        forked_from_id: row.get("forked_from_id")?,
        workspace_id: row.get("workspace_id")?,
    })
}

fn json_column(row: &Row<'_>, name: &str) -> rusqlite::Result<Option<Value>> {
    let raw: Option<String> = row.get(name)?;
    Ok(raw.and_then(|text| serde_json::from_str(&text).ok()))
}

fn message_from_row(row: &Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get("id")?,
        conversation_id: row.get("conversation_id")?,
        role: row.get("role")?,
        content: row.get("content")?,
        reasoning_content: row.get("reasoning_content")?,
        tool_calls: json_column(row, "tool_calls")?,
        tool_call_id: row.get("tool_call_id")?,
        attachments: json_column(row, "attachments")?,
        used_web_search: row.get::<_, i64>("used_web_search")? != 0,
        web_sources: json_column(row, "web_sources")?,
        model_id: row.get("model_id")?,
        usage_prompt_tokens: row.get("usage_prompt_tokens")?,
        usage_completion_tokens: row.get("usage_completion_tokens")?,
        timing_ttft_ms: row.get("timing_ttft_ms")?,
        timing_total_ms: row.get("timing_total_ms")?,
        timing_tokens_per_sec: row.get("timing_tokens_per_sec")?,
        finish_reason: row.get("finish_reason")?,
        created_at: row.get("created_at")?,
    })
}

fn encode(value: &Option<Value>) -> Result<Option<String>> {
    match value {
        Some(v) => Ok(Some(serde_json::to_string(v)?)),
        None => Ok(None),
    }
}

impl Db {
    pub fn create_conversation(&self, model_id: Option<&str>) -> Result<Conversation> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO conversations (id, title, title_mode, model_id, created_at, updated_at)
                 VALUES (?1, 'New chat', 'first_line', ?2, ?3, ?3)",
                params![id, model_id, timestamp],
            )?;
            Ok(())
        })?;

        self.get_conversation(&id)?
            .ok_or_else(|| crate::KuroError::other("conversation vanished after insert"))
    }

    pub fn get_conversation(&self, id: &str) -> Result<Option<Conversation>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT * FROM conversations WHERE id = ?1",
                    params![id],
                    conversation_from_row,
                )
                .optional()?;
            Ok(found)
        })
    }

    /// Conversations for the sidebar, newest activity first, pinned on top.
    ///
    /// `query` filters on title and message content so the sidebar search finds
    /// a chat by something that was said in it, not just its title.
    pub fn list_conversations(&self, query: Option<&str>) -> Result<Vec<Conversation>> {
        self.with(|conn| {
            let rows = match query.map(str::trim).filter(|q| !q.is_empty()) {
                Some(q) => {
                    let pattern = format!("%{}%", q);
                    let mut stmt = conn.prepare(
                        "SELECT DISTINCT c.* FROM conversations c
                         LEFT JOIN messages m ON m.conversation_id = c.id
                         WHERE c.archived = 0 AND (c.title LIKE ?1 OR m.content LIKE ?1)
                         ORDER BY c.pinned DESC, c.updated_at DESC",
                    )?;
                    let found = stmt
                        .query_map(params![pattern], conversation_from_row)?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    found
                }
                None => {
                    let mut stmt = conn.prepare(
                        "SELECT * FROM conversations WHERE archived = 0
                         ORDER BY pinned DESC, updated_at DESC",
                    )?;
                    let found = stmt
                        .query_map([], conversation_from_row)?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    found
                }
            };
            Ok(rows)
        })
    }

    /// Set the title. `manual` records that the user chose it, so automatic
    /// titling never overwrites it later.
    pub fn set_conversation_title(&self, id: &str, title: &str, manual: bool) -> Result<()> {
        let mode = if manual { "manual" } else { "first_line" };
        self.with(|conn| {
            conn.execute(
                "UPDATE conversations SET title = ?2, title_mode = ?3, updated_at = ?4
                 WHERE id = ?1",
                params![id, title, mode, now()],
            )?;
            Ok(())
        })
    }

    pub fn set_conversation_model(&self, id: &str, model_id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE conversations SET model_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, model_id, now()],
            )?;
            Ok(())
        })
    }

    pub fn set_conversation_flags(
        &self,
        id: &str,
        pinned: Option<bool>,
        archived: Option<bool>,
    ) -> Result<()> {
        self.with(|conn| {
            if let Some(pinned) = pinned {
                conn.execute(
                    "UPDATE conversations SET pinned = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, pinned as i64, now()],
                )?;
            }
            if let Some(archived) = archived {
                conn.execute(
                    "UPDATE conversations SET archived = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, archived as i64, now()],
                )?;
            }
            Ok(())
        })
    }

    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn insert_message(&self, conversation_id: &str, message: &NewMessage) -> Result<Message> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        let tool_calls = encode(&message.tool_calls)?;
        let attachments = encode(&message.attachments)?;
        let web_sources = encode(&message.web_sources)?;

        self.with(|conn| {
            conn.execute(
                "INSERT INTO messages (
                     id, conversation_id, role, content, reasoning_content, tool_calls,
                     tool_call_id, attachments, used_web_search, web_sources, model_id,
                     provider_slug, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    id,
                    conversation_id,
                    message.role,
                    message.content,
                    message.reasoning_content,
                    tool_calls,
                    message.tool_call_id,
                    attachments,
                    message.used_web_search as i64,
                    web_sources,
                    message.model_id,
                    message.provider_slug,
                    timestamp,
                ],
            )?;
            // Keep the sidebar ordered by real activity.
            conn.execute(
                "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
                params![conversation_id, timestamp],
            )?;
            Ok(())
        })?;

        self.get_message(&id)?
            .ok_or_else(|| crate::KuroError::other("message vanished after insert"))
    }

    pub fn get_message(&self, id: &str) -> Result<Option<Message>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT * FROM messages WHERE id = ?1",
                    params![id],
                    message_from_row,
                )
                .optional()?;
            Ok(found)
        })
    }

    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM messages WHERE conversation_id = ?1 ORDER BY created_at, rowid",
            )?;
            let rows = stmt
                .query_map(params![conversation_id], message_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Write the final text plus the usage and timing numbers that the request
    /// inspector shows.
    pub fn complete_message(&self, id: &str, completion: &MessageCompletion) -> Result<()> {
        let tool_calls = encode(&completion.tool_calls)?;
        let web_sources = encode(&completion.web_sources)?;

        self.with(|conn| {
            conn.execute(
                "UPDATE messages SET
                     content = ?2, reasoning_content = ?3,
                     usage_prompt_tokens = ?4, usage_completion_tokens = ?5,
                     timing_ttft_ms = ?6, timing_total_ms = ?7, timing_tokens_per_sec = ?8,
                     finish_reason = ?9, tool_calls = ?10,
                     used_web_search = ?11, web_sources = ?12,
                     usage_prompt_tokens_total = ?13
                 WHERE id = ?1",
                params![
                    id,
                    completion.content,
                    completion.reasoning_content,
                    completion.usage_prompt_tokens,
                    completion.usage_completion_tokens,
                    completion.timing_ttft_ms,
                    completion.timing_total_ms,
                    completion.timing_tokens_per_sec,
                    completion.finish_reason,
                    tool_calls,
                    completion.used_web_search as i64,
                    web_sources,
                    completion.usage_prompt_tokens_total,
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete_message(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    /// Delete `message_id` and everything after it in the conversation.
    ///
    /// This is what editing a message does: the turns that followed were answers
    /// to the old wording, so keeping them would leave a transcript in which the
    /// replies do not match what was asked. The cut-off is compared on
    /// `(created_at, rowid)`, the same ordering [`Db::list_messages`] reads back,
    /// so a message inserted in the same clock tick as its neighbour still cuts
    /// in the position the user actually saw it in.
    ///
    /// Returns how many messages were removed.
    pub fn delete_from(&self, conversation_id: &str, message_id: &str) -> Result<usize> {
        self.with(|conn| {
            // Checked rather than assumed: a message id from another conversation
            // would otherwise silently delete nothing and report success.
            let belongs: Option<String> = conn
                .query_row(
                    "SELECT conversation_id FROM messages WHERE id = ?1",
                    params![message_id],
                    |row| row.get(0),
                )
                .optional()?;

            match belongs.as_deref() {
                Some(owner) if owner == conversation_id => {}
                Some(_) => {
                    return Err(crate::KuroError::bad_request(format!(
                        "message `{message_id}` is not in conversation `{conversation_id}`"
                    )))
                }
                None => {
                    return Err(crate::KuroError::not_found(format!(
                        "message `{message_id}`"
                    )))
                }
            }

            let removed = conn.execute(
                "DELETE FROM messages
                 WHERE conversation_id = ?1
                   AND rowid IN (
                       SELECT m.rowid
                       FROM messages m, messages anchor
                       WHERE anchor.id = ?2
                         AND m.conversation_id = ?1
                         AND (m.created_at > anchor.created_at
                              OR (m.created_at = anchor.created_at AND m.rowid >= anchor.rowid))
                   )",
                params![conversation_id, message_id],
            )?;

            conn.execute(
                "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
                params![conversation_id, now()],
            )?;

            Ok(removed)
        })
    }

    /// Copy a conversation into a new one, up to and including `up_to_message_id`.
    ///
    /// `None` copies the whole thing. The copy carries the completed usage and
    /// tool trail of every assistant turn, not just its text, so the request
    /// inspector in a fork shows the same numbers the original does rather than
    /// an empty panel.
    pub fn fork_conversation(
        &self,
        source_id: &str,
        up_to_message_id: Option<&str>,
    ) -> Result<Conversation> {
        let source = self
            .get_conversation(source_id)?
            .ok_or_else(|| crate::KuroError::not_found(format!("conversation `{source_id}`")))?;

        let history = self.list_messages(source_id)?;

        let keep: Vec<&Message> = match up_to_message_id {
            None => history.iter().collect(),
            Some(cutoff) => {
                let end = history
                    .iter()
                    .position(|message| message.id == cutoff)
                    .ok_or_else(|| {
                        crate::KuroError::not_found(format!(
                            "message `{cutoff}` in conversation `{source_id}`"
                        ))
                    })?;
                history[..=end].iter().collect()
            }
        };

        let new_id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();

        self.with(|conn| {
            conn.execute(
                "INSERT INTO conversations (
                     id, title, title_mode, model_id, created_at, updated_at,
                     project_id, forked_from_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?5,
                     (SELECT project_id FROM conversations WHERE id = ?6), ?6)",
                params![
                    new_id,
                    source.title,
                    source.title_mode,
                    source.model_id,
                    timestamp,
                    source_id,
                ],
            )?;

            for message in &keep {
                // Original timestamps are kept so the copy reads back in exactly
                // the order it was said in, independent of how fast the rows are
                // written here.
                // `provider_slug` is deliberately not copied, which leaves it
                // NULL on every forked row. The usage totals sum over rows that
                // carry a slug, and a fork spent no allowance — copying it
                // would bill the user twice for one answer, and again for every
                // fork of the fork. The usage numbers below *are* copied,
                // because the request inspector shows them per message and a
                // fork should not look emptier than what it came from.
                conn.execute(
                    "INSERT INTO messages (
                         id, conversation_id, role, content, reasoning_content, tool_calls,
                         tool_call_id, attachments, used_web_search, web_sources, model_id,
                         usage_prompt_tokens, usage_completion_tokens, timing_ttft_ms,
                         timing_total_ms, timing_tokens_per_sec, finish_reason, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                               ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        new_id,
                        message.role,
                        message.content,
                        message.reasoning_content,
                        encode(&message.tool_calls)?,
                        message.tool_call_id,
                        encode(&message.attachments)?,
                        message.used_web_search as i64,
                        encode(&message.web_sources)?,
                        message.model_id,
                        message.usage_prompt_tokens,
                        message.usage_completion_tokens,
                        message.timing_ttft_ms,
                        message.timing_total_ms,
                        message.timing_tokens_per_sec,
                        message.finish_reason,
                        message.created_at,
                    ],
                )?;
            }

            Ok(())
        })?;

        self.get_conversation(&new_id)?
            .ok_or_else(|| crate::KuroError::other("conversation vanished after fork"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appending_a_message_bumps_conversation_activity() {
        let db = Db::open_in_memory().expect("open");
        let convo = db.create_conversation(Some("qwen3-4b:q4_k_m")).expect("create");
        let created_at = convo.updated_at.clone();

        db.insert_message(&convo.id, &NewMessage::user("hello"))
            .expect("insert");

        let reloaded = db.get_conversation(&convo.id).expect("get").expect("some");
        assert!(reloaded.updated_at >= created_at);
        assert_eq!(db.list_messages(&convo.id).expect("list").len(), 1);
    }

    #[test]
    fn deleting_a_conversation_removes_its_messages() {
        let db = Db::open_in_memory().expect("open");
        let convo = db.create_conversation(None).expect("create");
        db.insert_message(&convo.id, &NewMessage::user("hi")).expect("insert");

        db.delete_conversation(&convo.id).expect("delete");

        assert!(db.list_messages(&convo.id).expect("list").is_empty());
    }

    #[test]
    fn records_usage_and_timing_for_the_inspector() {
        let db = Db::open_in_memory().expect("open");
        let convo = db.create_conversation(None).expect("create");
        let message = db
            .insert_message(&convo.id, &NewMessage::assistant(""))
            .expect("insert");

        db.complete_message(
            &message.id,
            &MessageCompletion {
                content: "A black hole is...".to_string(),
                usage_prompt_tokens: Some(42),
                usage_completion_tokens: Some(181),
                timing_ttft_ms: Some(218),
                timing_tokens_per_sec: Some(37.4),
                finish_reason: Some("stop".to_string()),
                ..Default::default()
            },
        )
        .expect("complete");

        let stored = db.get_message(&message.id).expect("get").expect("some");
        assert_eq!(stored.content, "A black hole is...");
        assert_eq!(stored.usage_completion_tokens, Some(181));
        assert_eq!(stored.timing_tokens_per_sec, Some(37.4));
    }

    #[test]
    fn search_matches_message_content_not_just_titles() {
        let db = Db::open_in_memory().expect("open");
        let convo = db.create_conversation(None).expect("create");
        db.insert_message(&convo.id, &NewMessage::user("explain rust lifetimes"))
            .expect("insert");

        let hits = db.list_conversations(Some("lifetimes")).expect("search");
        assert_eq!(hits.len(), 1);

        let misses = db.list_conversations(Some("kubernetes")).expect("search");
        assert!(misses.is_empty());
    }

    #[test]
    fn archived_conversations_stay_out_of_the_sidebar() {
        let db = Db::open_in_memory().expect("open");
        let convo = db.create_conversation(None).expect("create");
        db.set_conversation_flags(&convo.id, None, Some(true)).expect("archive");

        assert!(db.list_conversations(None).expect("list").is_empty());
    }

    /// A conversation with four alternating turns, returned with its messages.
    fn conversation_with_history(db: &Db) -> (Conversation, Vec<Message>) {
        let convo = db.create_conversation(Some("qwen3-4b:q4_k_m")).expect("create");
        for text in ["first", "reply one", "second", "reply two"] {
            let role = if text.starts_with("reply") {
                NewMessage::assistant(text)
            } else {
                NewMessage::user(text)
            };
            db.insert_message(&convo.id, &role).expect("insert");
        }
        let history = db.list_messages(&convo.id).expect("list");
        (convo, history)
    }

    #[test]
    fn editing_a_message_cuts_it_and_everything_after_it() {
        let db = Db::open_in_memory().expect("open");
        let (convo, history) = conversation_with_history(&db);

        // Edit the second user turn: it and both following rows must go.
        let removed = db.delete_from(&convo.id, &history[2].id).expect("truncate");

        assert_eq!(removed, 2, "the edited turn and the reply to it");
        let left = db.list_messages(&convo.id).expect("list");
        assert_eq!(
            left.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
            vec!["first", "reply one"]
        );
    }

    #[test]
    fn truncating_is_ordered_by_position_not_just_timestamp() {
        // Every row here is written inside the same clock tick, so `created_at`
        // alone cannot separate them. The cut must still land where the user saw
        // the message, which is what the rowid tie-break is for.
        let db = Db::open_in_memory().expect("open");
        let convo = db.create_conversation(None).expect("create");
        let stamp = now();
        for (index, text) in ["one", "two", "three"].iter().enumerate() {
            db.with(|conn| {
                conn.execute(
                    "INSERT INTO messages (id, conversation_id, role, content, created_at)
                     VALUES (?1, ?2, 'user', ?3, ?4)",
                    params![format!("m{index}"), convo.id, text, stamp],
                )?;
                Ok(())
            })
            .expect("insert");
        }

        db.delete_from(&convo.id, "m1").expect("truncate");

        let left = db.list_messages(&convo.id).expect("list");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].content, "one");
    }

    #[test]
    fn truncating_refuses_a_message_from_another_conversation() {
        let db = Db::open_in_memory().expect("open");
        let (first, history) = conversation_with_history(&db);
        let second = db.create_conversation(None).expect("create");
        db.insert_message(&second.id, &NewMessage::user("unrelated"))
            .expect("insert");

        // Silently deleting nothing would look like success to the caller.
        assert!(db.delete_from(&second.id, &history[0].id).is_err());
        assert!(db.delete_from(&first.id, "no-such-message").is_err());
        assert_eq!(db.list_messages(&second.id).expect("list").len(), 1);
    }

    #[test]
    fn forking_copies_history_up_to_the_chosen_message() {
        let db = Db::open_in_memory().expect("open");
        let (source, history) = conversation_with_history(&db);

        let fork = db
            .fork_conversation(&source.id, Some(&history[1].id))
            .expect("fork");

        assert_eq!(fork.forked_from_id.as_deref(), Some(source.id.as_str()));
        assert_eq!(fork.model_id, source.model_id);
        assert_eq!(fork.title, source.title);

        let copied = db.list_messages(&fork.id).expect("list");
        assert_eq!(
            copied.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
            vec!["first", "reply one"],
            "the cutoff message is included, nothing after it is"
        );
        assert!(
            copied.iter().all(|m| m.conversation_id == fork.id),
            "copies must belong to the fork"
        );
        assert!(
            copied.iter().all(|m| !history.iter().any(|old| old.id == m.id)),
            "copies must have their own ids, or editing one would edit the original"
        );
        assert_eq!(
            db.list_messages(&source.id).expect("list").len(),
            4,
            "the original is left untouched"
        );
    }

    #[test]
    fn a_fork_carries_the_usage_numbers_the_inspector_shows() {
        let db = Db::open_in_memory().expect("open");
        let source = db.create_conversation(None).expect("create");
        db.insert_message(&source.id, &NewMessage::user("explain")).expect("insert");
        let assistant = db
            .insert_message(&source.id, &NewMessage::assistant(""))
            .expect("insert");
        db.complete_message(
            &assistant.id,
            &MessageCompletion {
                content: "because".to_string(),
                usage_completion_tokens: Some(181),
                timing_tokens_per_sec: Some(37.4),
                finish_reason: Some("stop".to_string()),
                ..Default::default()
            },
        )
        .expect("complete");

        let fork = db.fork_conversation(&source.id, None).expect("fork");

        let copied = db.list_messages(&fork.id).expect("list");
        assert_eq!(copied.len(), 2);
        assert_eq!(copied[1].content, "because");
        assert_eq!(copied[1].usage_completion_tokens, Some(181));
        assert_eq!(copied[1].timing_tokens_per_sec, Some(37.4));
        assert_eq!(copied[1].finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn deleting_the_original_leaves_the_fork_alone() {
        let db = Db::open_in_memory().expect("open");
        let (source, _) = conversation_with_history(&db);
        let fork = db.fork_conversation(&source.id, None).expect("fork");

        db.delete_conversation(&source.id).expect("delete");

        let survived = db.get_conversation(&fork.id).expect("get").expect("some");
        assert_eq!(survived.forked_from_id, None, "the link is released, not the chat");
        assert_eq!(db.list_messages(&fork.id).expect("list").len(), 4);
    }

    #[test]
    fn forking_at_an_unknown_message_is_an_error_not_a_full_copy() {
        let db = Db::open_in_memory().expect("open");
        let (source, _) = conversation_with_history(&db);

        assert!(db.fork_conversation(&source.id, Some("no-such-message")).is_err());
        assert!(db.fork_conversation("no-such-conversation", None).is_err());
    }
}
