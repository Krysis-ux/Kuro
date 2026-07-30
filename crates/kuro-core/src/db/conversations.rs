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
    pub usage_completion_tokens: Option<i64>,
    pub timing_ttft_ms: Option<i64>,
    pub timing_total_ms: Option<i64>,
    pub timing_tokens_per_sec: Option<f64>,
    pub finish_reason: Option<String>,
}

fn conversation_from_row(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get("id")?,
        title: row.get("title")?,
        title_mode: row.get("title_mode")?,
        model_id: row.get("model_id")?,
        pinned: row.get::<_, i64>("pinned")? != 0,
        archived: row.get::<_, i64>("archived")? != 0,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
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
                     tool_call_id, attachments, used_web_search, web_sources, model_id, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
        self.with(|conn| {
            conn.execute(
                "UPDATE messages SET
                     content = ?2, reasoning_content = ?3,
                     usage_prompt_tokens = ?4, usage_completion_tokens = ?5,
                     timing_ttft_ms = ?6, timing_total_ms = ?7, timing_tokens_per_sec = ?8,
                     finish_reason = ?9
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
}
