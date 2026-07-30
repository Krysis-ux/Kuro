//! Storage for what the model has been asked to remember.
//!
//! Recall is a substring match rather than an embedding search. That is a
//! deliberate first version: it needs no second model resident in memory, it is
//! predictable enough that a user can guess why something did or did not come
//! back, and the table is small enough that scanning it costs nothing. Swapping
//! in vector search later only changes [`Db::recall_memories`].

use rusqlite::{params, Row};
use serde::Serialize;

use super::{json_or, now, Db};
use crate::Result;

/// Upper bound on how much one memory can hold, so a tool call cannot paste an
/// entire document into the recall context of every later conversation.
const MAX_CONTENT_CHARS: usize = 2000;

#[derive(Debug, Clone, Serialize)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub created_at: String,
}

impl Db {
    /// Store a memory, or return the existing one if the same text is already
    /// held. A model told the same fact twice should not accumulate duplicates.
    pub fn remember(
        &self,
        content: &str,
        tags: &[String],
        source: Option<&str>,
    ) -> Result<MemoryRecord> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(crate::KuroError::bad_request("there is nothing to remember"));
        }

        let content: String = trimmed.chars().take(MAX_CONTENT_CHARS).collect();

        if let Some(existing) = self.find_memory_by_content(&content)? {
            return Ok(existing);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let encoded_tags = serde_json::to_string(tags)?;
        let created_at = now();

        self.with(|conn| {
            conn.execute(
                "INSERT INTO memories (id, content, tags, source, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, content, encoded_tags, source, created_at],
            )?;
            Ok(())
        })?;

        Ok(MemoryRecord {
            id,
            content,
            tags: tags.to_vec(),
            source: source.map(str::to_string),
            created_at,
        })
    }

    fn find_memory_by_content(&self, content: &str) -> Result<Option<MemoryRecord>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, content, tags, source, created_at
                   FROM memories WHERE content = ?1 LIMIT 1",
            )?;
            let mut rows = stmt.query_map(params![content], read_memory)?;
            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    pub fn list_memories(&self, limit: usize) -> Result<Vec<MemoryRecord>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, content, tags, source, created_at
                   FROM memories ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt
                .query_map(params![limit as i64], read_memory)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Memories matching every word in the query, newest first.
    ///
    /// Requiring all words rather than any keeps a two-word question from
    /// returning most of the table.
    pub fn recall_memories(&self, query: &str, limit: usize) -> Result<Vec<MemoryRecord>> {
        let words: Vec<String> = query
            .split_whitespace()
            .filter(|word| word.len() > 1)
            .map(|word| word.to_lowercase())
            .collect();

        if words.is_empty() {
            return self.list_memories(limit);
        }

        let all = self.list_memories(500)?;
        Ok(all
            .into_iter()
            .filter(|memory| {
                let haystack = memory.content.to_lowercase();
                let tags = memory.tags.join(" ").to_lowercase();
                words
                    .iter()
                    .all(|word| haystack.contains(word) || tags.contains(word))
            })
            .take(limit)
            .collect())
    }

    pub fn forget_memory(&self, id: &str) -> Result<bool> {
        self.with(|conn| {
            let removed = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
            Ok(removed > 0)
        })
    }

    pub fn count_memories(&self) -> Result<i64> {
        self.with(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?)
        })
    }
}

fn read_memory(row: &Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let tags: Option<String> = row.get(2)?;
    Ok(MemoryRecord {
        id: row.get(0)?,
        content: row.get(1)?,
        tags: json_or(tags.as_deref()),
        source: row.get(3)?,
        created_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_lists_a_memory() {
        let db = Db::open_in_memory().expect("open");
        let stored = db
            .remember("Lennon prefers terse commit messages", &["style".to_string()], Some("c1"))
            .expect("remember");

        assert_eq!(stored.tags, vec!["style".to_string()]);
        assert_eq!(stored.source.as_deref(), Some("c1"));
        assert_eq!(db.count_memories().expect("count"), 1);
        assert_eq!(db.list_memories(10).expect("list").len(), 1);
    }

    #[test]
    fn the_same_fact_twice_does_not_duplicate() {
        let db = Db::open_in_memory().expect("open");
        let first = db.remember("the deploy script lives in ops/", &[], None).expect("first");
        let second = db.remember("  the deploy script lives in ops/  ", &[], None).expect("second");

        assert_eq!(first.id, second.id);
        assert_eq!(db.count_memories().expect("count"), 1);
    }

    #[test]
    fn recall_requires_every_word_to_match() {
        let db = Db::open_in_memory().expect("open");
        db.remember("the staging database is in Frankfurt", &[], None).expect("a");
        db.remember("the production database is in Oregon", &[], None).expect("b");

        let staging = db.recall_memories("staging database", 10).expect("recall");
        assert_eq!(staging.len(), 1);
        assert!(staging[0].content.contains("Frankfurt"));

        let both = db.recall_memories("database", 10).expect("recall");
        assert_eq!(both.len(), 2);

        let neither = db.recall_memories("staging Oregon", 10).expect("recall");
        assert!(neither.is_empty(), "words from different memories must not match either");
    }

    #[test]
    fn recall_also_searches_tags() {
        let db = Db::open_in_memory().expect("open");
        db.remember("prefers tabs", &["formatting".to_string()], None).expect("a");

        assert_eq!(db.recall_memories("formatting", 10).expect("recall").len(), 1);
    }

    #[test]
    fn an_empty_query_returns_the_most_recent() {
        let db = Db::open_in_memory().expect("open");
        db.remember("one", &[], None).expect("a");
        db.remember("two", &[], None).expect("b");

        assert_eq!(db.recall_memories("   ", 10).expect("recall").len(), 2);
    }

    #[test]
    fn refuses_to_store_nothing() {
        let db = Db::open_in_memory().expect("open");
        assert!(db.remember("   \n ", &[], None).is_err());
    }

    #[test]
    fn oversized_content_is_truncated_rather_than_rejected() {
        let db = Db::open_in_memory().expect("open");
        let stored = db.remember(&"x".repeat(MAX_CONTENT_CHARS + 500), &[], None).expect("remember");
        assert_eq!(stored.content.chars().count(), MAX_CONTENT_CHARS);
    }

    #[test]
    fn forgetting_reports_whether_it_removed_anything() {
        let db = Db::open_in_memory().expect("open");
        let stored = db.remember("temporary", &[], None).expect("remember");

        assert!(db.forget_memory(&stored.id).expect("forget"));
        assert!(!db.forget_memory(&stored.id).expect("again"));
        assert_eq!(db.count_memories().expect("count"), 0);
    }
}
