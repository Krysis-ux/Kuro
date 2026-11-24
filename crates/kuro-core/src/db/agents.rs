//! Storage for subagents the user wrote. Separate from the built-in list
//! ([`crate::agents::AGENTS`]) because these must be editable and removable.

use rusqlite::{params, Row};
use serde::Serialize;

use super::{now, Db};
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserAgentRecord {
    pub slug: String,
    pub name: String,
    pub blurb: String,
    /// When the delegating model should pick this one.
    pub when: String,
    /// What the agent is told it is, ahead of the workspace brief.
    pub brief: String,
    /// Skill slugs loaded on top of whatever the project justifies.
    pub skills: Vec<String>,
    /// Whether it may change files.
    pub writes: bool,
}

impl Db {
    pub fn list_user_agents(&self) -> Result<Vec<UserAgentRecord>> {
        self.with(|conn| {
            let mut statement = conn.prepare(
                "SELECT slug, name, blurb, when_to_use, brief, skills, writes
                 FROM user_agents
                 ORDER BY name COLLATE NOCASE",
            )?;
            let rows = statement.query_map([], read)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Store an agent, replacing one of the same slug.
    pub fn put_user_agent(&self, agent: &UserAgentRecord) -> Result<()> {
        let skills = serde_json::to_string(&agent.skills)?;
        let stamp = now();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO user_agents
                     (slug, name, blurb, when_to_use, brief, skills, writes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT(slug) DO UPDATE SET
                     name = excluded.name,
                     blurb = excluded.blurb,
                     when_to_use = excluded.when_to_use,
                     brief = excluded.brief,
                     skills = excluded.skills,
                     writes = excluded.writes,
                     updated_at = excluded.updated_at",
                params![
                    agent.slug,
                    agent.name,
                    agent.blurb,
                    agent.when,
                    agent.brief,
                    skills,
                    agent.writes,
                    stamp,
                ],
            )?;
            Ok(())
        })
    }

    /// Remove an agent. Returns false when there was none by that slug.
    pub fn delete_user_agent(&self, slug: &str) -> Result<bool> {
        self.with(|conn| {
            let removed = conn.execute("DELETE FROM user_agents WHERE slug = ?1", params![slug])?;
            Ok(removed > 0)
        })
    }
}

fn read(row: &Row<'_>) -> rusqlite::Result<UserAgentRecord> {
    let skills: String = row.get("skills")?;
    Ok(UserAgentRecord {
        slug: row.get("slug")?,
        name: row.get("name")?,
        blurb: row.get("blurb")?,
        when: row.get("when_to_use")?,
        brief: row.get("brief")?,
        // A hand-edited row with bad JSON loses its skills rather than the agent.
        skills: serde_json::from_str(&skills).unwrap_or_default(),
        writes: row.get("writes")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(slug: &str) -> UserAgentRecord {
        UserAgentRecord {
            slug: slug.to_string(),
            name: "Docs writer".to_string(),
            blurb: "Writes documentation.".to_string(),
            when: "Use for READMEs.".to_string(),
            brief: "You write docs.".to_string(),
            skills: vec!["testing".to_string()],
            writes: true,
        }
    }

    #[test]
    fn stores_updates_and_removes_an_agent() {
        let db = Db::open_in_memory().unwrap();
        db.put_user_agent(&record("docs")).unwrap();
        assert_eq!(db.list_user_agents().unwrap(), vec![record("docs")]);

        let changed = UserAgentRecord { writes: false, skills: vec![], ..record("docs") };
        db.put_user_agent(&changed).unwrap();
        assert_eq!(db.list_user_agents().unwrap(), vec![changed]);

        assert!(db.delete_user_agent("docs").unwrap());
        assert!(!db.delete_user_agent("docs").unwrap());
        assert!(db.list_user_agents().unwrap().is_empty());
    }
}
