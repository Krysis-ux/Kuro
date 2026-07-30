//! Raw settings key/value storage.
//!
//! Values are JSON so a setting can be a bool, number, string or object without
//! schema changes. Typed accessors and defaults live in [`crate::settings`].
//!
//! Secrets (API keys, cloud credentials) are deliberately *not* stored here —
//! they go to the macOS Keychain, and only a "configured" flag lands in this
//! table.

use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use super::{now, Db};
use crate::Result;

impl Db {
    pub fn get_setting(&self, key: &str) -> Result<Option<Value>> {
        self.with(|conn| {
            let raw: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()?;

            match raw {
                Some(text) => Ok(serde_json::from_str(&text).ok()),
                None => Ok(None),
            }
        })
    }

    pub fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        let encoded = serde_json::to_string(value)?;
        self.with(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value,
                                                 updated_at = excluded.updated_at",
                params![key, encoded, now()],
            )?;
            Ok(())
        })
    }

    /// Every stored setting. Keys absent here fall back to code defaults.
    pub fn all_settings(&self) -> Result<serde_json::Map<String, Value>> {
        self.with(|conn| {
            let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
            let rows = stmt
                .query_map([], |row| {
                    let key: String = row.get(0)?;
                    let value: String = row.get(1)?;
                    Ok((key, value))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let mut map = serde_json::Map::new();
            for (key, value) in rows {
                if let Ok(parsed) = serde_json::from_str(&value) {
                    map.insert(key, parsed);
                }
            }
            Ok(map)
        })
    }

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stores_and_overwrites_typed_values() {
        let db = Db::open_in_memory().expect("open");
        assert_eq!(db.get_setting("theme").expect("get"), None);

        db.set_setting("theme", &json!("dark")).expect("set");
        assert_eq!(db.get_setting("theme").expect("get"), Some(json!("dark")));

        db.set_setting("theme", &json!("light")).expect("overwrite");
        assert_eq!(db.get_setting("theme").expect("get"), Some(json!("light")));

        db.set_setting("server", &json!({"port": 8420}))
            .expect("object");
        let all = db.all_settings().expect("all");
        assert_eq!(all["server"]["port"], json!(8420));
        assert_eq!(all.len(), 2);

        db.delete_setting("theme").expect("delete");
        assert_eq!(db.get_setting("theme").expect("get"), None);
    }
}
