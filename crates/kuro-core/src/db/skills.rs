//! Storage for skills the user added themselves.
//!
//! Separate from the built-in catalogue, which is part of the build and lives in
//! [`crate::skills::SKILLS`]. The difference is not where the text sits, it is
//! what may be done to it: a built-in cannot be deleted and does not need to be,
//! and one of these can turn out to be nonsense on import and has to be
//! removable in one click.

use rusqlite::{params, Row};
use serde::Serialize;

use super::{now, Db};
use crate::Result;

/// Upper bound on one skill's instructions.
///
/// A skill is appended to the system prompt, so its size is paid on every turn
/// it is on. Some repositories keep a whole reference manual in a `SKILL.md`;
/// that is a document, not a skill, and pasting one into every prompt is how a
/// context window disappears without anybody choosing to spend it.
pub const MAX_INSTRUCTION_CHARS: usize = 12_000;

#[derive(Debug, Clone, Serialize)]
pub struct UserSkillRecord {
    pub slug: String,
    pub name: String,
    pub blurb: String,
    pub category: String,
    pub instructions: String,
    pub approx_tokens: usize,
    /// `upload`, or the repository it was pulled from.
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Db {
    pub fn list_user_skills(&self) -> Result<Vec<UserSkillRecord>> {
        self.with(|conn| {
            let mut statement = conn.prepare(
                "SELECT slug, name, blurb, category, instructions, approx_tokens, source,
                        created_at, updated_at
                 FROM user_skills
                 ORDER BY name COLLATE NOCASE",
            )?;
            let rows = statement.query_map([], read)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn get_user_skill(&self, slug: &str) -> Result<Option<UserSkillRecord>> {
        self.with(|conn| {
            let mut statement = conn.prepare(
                "SELECT slug, name, blurb, category, instructions, approx_tokens, source,
                        created_at, updated_at
                 FROM user_skills WHERE slug = ?1",
            )?;
            let mut rows = statement.query_map(params![slug], read)?;
            Ok(rows.next().transpose()?)
        })
    }

    /// Store a skill, replacing one of the same slug.
    ///
    /// Replacing rather than refusing, because re-importing a repository is how
    /// somebody updates a skill they got from it, and the alternative is asking
    /// them to delete twenty rows by hand first.
    pub fn put_user_skill(&self, skill: &UserSkillRecord) -> Result<()> {
        let stamp = now();
        self.with(|conn| {
            conn.execute(
            "INSERT INTO user_skills
                 (slug, name, blurb, category, instructions, approx_tokens, source,
                  created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(slug) DO UPDATE SET
                 name = excluded.name,
                 blurb = excluded.blurb,
                 category = excluded.category,
                 instructions = excluded.instructions,
                 approx_tokens = excluded.approx_tokens,
                 source = excluded.source,
                 updated_at = excluded.updated_at",
            params![
                skill.slug,
                skill.name,
                skill.blurb,
                skill.category,
                skill.instructions,
                skill.approx_tokens as i64,
                skill.source,
                stamp,
            ],
            )?;
            Ok(())
        })
    }

    /// Remove a skill. Returns false when there was none by that slug.
    pub fn delete_user_skill(&self, slug: &str) -> Result<bool> {
        self.with(|conn| {
            let removed =
                conn.execute("DELETE FROM user_skills WHERE slug = ?1", params![slug])?;
            Ok(removed > 0)
        })
    }
}

fn read(row: &Row<'_>) -> rusqlite::Result<UserSkillRecord> {
    Ok(UserSkillRecord {
        slug: row.get("slug")?,
        name: row.get("name")?,
        blurb: row.get("blurb")?,
        category: row.get("category")?,
        instructions: row.get("instructions")?,
        approx_tokens: row.get::<_, i64>("approx_tokens")?.max(0) as usize,
        source: row.get("source")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(slug: &str, name: &str) -> UserSkillRecord {
        UserSkillRecord {
            slug: slug.to_string(),
            name: name.to_string(),
            blurb: "does a thing".to_string(),
            category: "practice".to_string(),
            instructions: "Do the thing.".to_string(),
            approx_tokens: 12,
            source: "upload".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn a_skill_is_stored_read_back_and_removed() {
        let db = Db::open_in_memory().expect("db");

        db.put_user_skill(&record("taste", "Taste")).expect("put");
        let held = db.list_user_skills().expect("list");
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].name, "Taste");

        assert!(db.delete_user_skill("taste").expect("delete"));
        assert!(db.list_user_skills().expect("list").is_empty());
        assert!(!db.delete_user_skill("taste").expect("delete"), "already gone");
    }

    #[test]
    fn re_importing_replaces_rather_than_duplicating() {
        // Pulling the same repository again is how a skill gets updated. Twenty
        // duplicate rows would be the alternative.
        let db = Db::open_in_memory().expect("db");

        db.put_user_skill(&record("taste", "Taste")).expect("put");
        let mut second = record("taste", "Taste, revised");
        second.instructions = "Do the thing better.".to_string();
        db.put_user_skill(&second).expect("put");

        let held = db.list_user_skills().expect("list");
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].name, "Taste, revised");
        assert_eq!(held[0].instructions, "Do the thing better.");
    }
}
