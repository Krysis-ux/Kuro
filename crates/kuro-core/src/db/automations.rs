//! Automations: saved prompts that run on a schedule. The runs themselves are
//! ordinary chat turns (see `kuro-server`'s `routes::automations`); this file
//! stores the automations and decides when each is next due.

use std::fmt;

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, SecondsFormat, TimeZone, Utc, Weekday};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{now, Db};
use crate::{KuroError, Result};

/// The shortest repeat allowed: each run is a whole turn, often a billed one.
const MIN_EVERY_MINUTES: u32 = 5;
/// A week. Longer belongs to a calendar.
const MAX_EVERY_MINUTES: u32 = 7 * 24 * 60;

/// When an automation runs. Times are this Mac's local time, since the server
/// runs on it and "daily at 9" means 9 where the user is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schedule {
    Every { minutes: u32 },
    Daily { at: NaiveTime },
    Weekdays { at: NaiveTime },
}

impl Schedule {
    /// Read `every:30`, `daily:09:00` or `weekdays:09:00`.
    pub fn parse(text: &str) -> Result<Self> {
        let invalid = || {
            KuroError::bad_request(format!(
                "`{text}` is not a schedule. Use every:MINUTES, daily:HH:MM or weekdays:HH:MM."
            ))
        };
        let (kind, value) = text.trim().split_once(':').ok_or_else(invalid)?;
        let time = || NaiveTime::parse_from_str(value, "%H:%M").map_err(|_| invalid());

        match kind {
            "every" => {
                let minutes: u32 = value.parse().map_err(|_| invalid())?;
                if !(MIN_EVERY_MINUTES..=MAX_EVERY_MINUTES).contains(&minutes) {
                    return Err(KuroError::bad_request(format!(
                        "Repeat every {MIN_EVERY_MINUTES} minutes to {MAX_EVERY_MINUTES} minutes (a week)."
                    )));
                }
                Ok(Self::Every { minutes })
            }
            "daily" => Ok(Self::Daily { at: time()? }),
            "weekdays" => Ok(Self::Weekdays { at: time()? }),
            _ => Err(invalid()),
        }
    }

    /// The first run strictly after `after`.
    ///
    /// Callers pass the time a run started (or now), never the slot it was due
    /// in, so a Mac that slept through several slots runs once on waking rather
    /// than once per slot.
    pub fn next_after<Tz: TimeZone>(&self, after: &DateTime<Tz>) -> DateTime<Tz> {
        let (at, weekdays_only) = match *self {
            Self::Every { minutes } => return after.clone() + Duration::minutes(minutes.into()),
            Self::Daily { at } => (at, false),
            Self::Weekdays { at } => (at, true),
        };

        let zone = after.timezone();
        let mut day = after.date_naive();
        // Today, then the next week of days: enough to reach a weekday from a Friday evening.
        for _ in 0..8 {
            let workday = !matches!(day.weekday(), Weekday::Sat | Weekday::Sun);
            if workday || !weekdays_only {
                let wall = day.and_time(at);
                // A time skipped by a clock change runs an hour later rather than not at all.
                let candidate = zone
                    .from_local_datetime(&wall)
                    .earliest()
                    .or_else(|| zone.from_local_datetime(&(wall + Duration::hours(1))).earliest());
                if let Some(candidate) = candidate.filter(|candidate| candidate > after) {
                    return candidate;
                }
            }
            day = day.succ_opt().unwrap_or(day);
        }
        after.clone() + Duration::days(1)
    }
}

impl fmt::Display for Schedule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Every { minutes } => write!(f, "every:{minutes}"),
            Self::Daily { at } => write!(f, "daily:{}", at.format("%H:%M")),
            Self::Weekdays { at } => write!(f, "weekdays:{}", at.format("%H:%M")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutomationRecord {
    pub id: String,
    pub name: String,
    pub prompt: String,
    /// `None` runs on whatever the server's default model is at the time.
    pub model: Option<String>,
    /// As [`Schedule`] writes it.
    pub schedule: String,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    /// `None` while switched off.
    pub next_run_at: Option<String>,
    /// Why the last run failed, if it did.
    pub last_error: Option<String>,
    /// The chat each run's reply lands in.
    pub conversation_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// What the user writes: everything but the bookkeeping.
#[derive(Debug, Clone, Deserialize)]
pub struct AutomationInput {
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub model: Option<String>,
    pub schedule: String,
    #[serde(default = "switched_on")]
    pub enabled: bool,
}

fn switched_on() -> bool {
    true
}

impl AutomationInput {
    /// Refuse what cannot run, and tidy what can.
    fn checked(&self) -> Result<(AutomationInput, Schedule)> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(KuroError::bad_request("Give the automation a name."));
        }
        if self.prompt.trim().is_empty() {
            return Err(KuroError::bad_request("Write the prompt it should send."));
        }
        let schedule = Schedule::parse(&self.schedule)?;
        let model = self
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty() && *model != "default")
            .map(str::to_string);
        Ok((
            AutomationInput {
                name: name.to_string(),
                prompt: self.prompt.clone(),
                model,
                schedule: schedule.to_string(),
                enabled: self.enabled,
            },
            schedule,
        ))
    }
}

/// When a switched-on automation next runs, counted from now; `None` when off.
fn next_run(schedule: Schedule, enabled: bool) -> Option<String> {
    enabled.then(|| stamp(schedule.next_after(&Local::now())))
}

/// Whole seconds in UTC, so stored times compare cleanly.
fn stamp<Tz: TimeZone>(at: DateTime<Tz>) -> String {
    at.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Secs, true)
}

impl Db {
    pub fn list_automations(&self) -> Result<Vec<AutomationRecord>> {
        self.with(|conn| {
            let mut statement =
                conn.prepare("SELECT * FROM user_automations ORDER BY name COLLATE NOCASE, created_at")?;
            let rows = statement.query_map([], read)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn get_automation(&self, id: &str) -> Result<Option<AutomationRecord>> {
        self.with(|conn| {
            Ok(conn
                .query_row("SELECT * FROM user_automations WHERE id = ?1", params![id], read)
                .optional()?)
        })
    }

    pub fn create_automation(&self, input: &AutomationInput) -> Result<AutomationRecord> {
        let (input, schedule) = input.checked()?;
        let id = uuid::Uuid::new_v4().to_string();
        let stamp = now();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO user_automations
                     (id, name, prompt, model, schedule, enabled, next_run_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![
                    id,
                    input.name,
                    input.prompt,
                    input.model,
                    input.schedule,
                    input.enabled,
                    next_run(schedule, input.enabled),
                    stamp,
                ],
            )?;
            Ok(())
        })?;
        self.get_automation(&id)?
            .ok_or_else(|| KuroError::other("an automation vanished as it was saved"))
    }

    /// Replace what the user wrote. The next run is counted afresh from now, so
    /// switching one back on after a week does not fire it at once. `None` when
    /// there is no such automation.
    pub fn update_automation(&self, id: &str, input: &AutomationInput) -> Result<Option<AutomationRecord>> {
        let (input, schedule) = input.checked()?;
        let changed = self.with(|conn| {
            Ok(conn.execute(
                "UPDATE user_automations
                 SET name = ?2, prompt = ?3, model = ?4, schedule = ?5, enabled = ?6,
                     next_run_at = ?7, updated_at = ?8
                 WHERE id = ?1",
                params![
                    id,
                    input.name,
                    input.prompt,
                    input.model,
                    input.schedule,
                    input.enabled,
                    next_run(schedule, input.enabled),
                    now(),
                ],
            )?)
        })?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_automation(id)
    }

    /// Remove an automation. Its chat stays. False when there was none.
    pub fn delete_automation(&self, id: &str) -> Result<bool> {
        self.with(|conn| Ok(conn.execute("DELETE FROM user_automations WHERE id = ?1", params![id])? > 0))
    }

    /// Switched-on automations whose next run is at or before `now`.
    pub fn due_automations(&self, now: DateTime<Utc>) -> Result<Vec<AutomationRecord>> {
        Ok(self
            .list_automations()?
            .into_iter()
            .filter(|automation| automation.enabled)
            .filter(|automation| {
                automation
                    .next_run_at
                    .as_deref()
                    .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                    .is_some_and(|at| at <= now)
            })
            .collect())
    }

    /// Note that a run started at `started` into `conversation_id`, and count the
    /// next run from then. Clears the last error: this run has its own outcome.
    pub fn record_automation_run(
        &self,
        id: &str,
        conversation_id: &str,
        started: DateTime<Local>,
    ) -> Result<()> {
        let automation = self
            .get_automation(id)?
            .ok_or_else(|| KuroError::not_found(format!("automation `{id}`")))?;
        let schedule = Schedule::parse(&automation.schedule)?;
        let next = automation.enabled.then(|| stamp(schedule.next_after(&started)));
        self.with(|conn| {
            conn.execute(
                "UPDATE user_automations
                 SET last_run_at = ?2, next_run_at = ?3, conversation_id = ?4, last_error = NULL
                 WHERE id = ?1",
                params![id, stamp(started), next, conversation_id],
            )?;
            Ok(())
        })
    }

    /// Record how the last run ended: `None` for a reply, else why there was none.
    pub fn set_automation_error(&self, id: &str, error: Option<&str>) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE user_automations SET last_error = ?2 WHERE id = ?1",
                params![id, error],
            )?;
            Ok(())
        })
    }
}

fn read(row: &Row<'_>) -> rusqlite::Result<AutomationRecord> {
    Ok(AutomationRecord {
        id: row.get("id")?,
        name: row.get("name")?,
        prompt: row.get("prompt")?,
        model: row.get("model")?,
        schedule: row.get("schedule")?,
        enabled: row.get("enabled")?,
        last_run_at: row.get("last_run_at")?,
        next_run_at: row.get("next_run_at")?,
        last_error: row.get("last_error")?,
        conversation_id: row.get("conversation_id")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    /// 2026-11-20 is a Friday.
    fn at(day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(9 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 11, day, hour, minute, 0)
            .unwrap()
    }

    fn input(schedule: &str) -> AutomationInput {
        AutomationInput {
            name: "  Morning news ".to_string(),
            prompt: "Summarise today's news.".to_string(),
            model: None,
            schedule: schedule.to_string(),
            enabled: true,
        }
    }

    #[test]
    fn reads_and_writes_each_kind_of_schedule() {
        for text in ["every:30", "daily:09:00", "weekdays:18:45"] {
            assert_eq!(Schedule::parse(text).unwrap().to_string(), text);
        }
        assert_eq!(Schedule::parse(" daily:7:05 ").unwrap().to_string(), "daily:07:05");
    }

    #[test]
    fn refuses_what_is_not_a_schedule() {
        for text in ["", "hourly", "every:x", "every:1", "every:999999", "daily:25:00", "daily:9", "monthly:09:00"] {
            assert!(Schedule::parse(text).is_err(), "{text:?} should be refused");
        }
    }

    #[test]
    fn every_counts_from_the_time_given() {
        let every = Schedule::parse("every:30").unwrap();
        assert_eq!(every.next_after(&at(20, 10, 0)), at(20, 10, 30));
    }

    #[test]
    fn daily_runs_later_today_or_else_tomorrow() {
        let daily = Schedule::parse("daily:09:00").unwrap();
        assert_eq!(daily.next_after(&at(20, 8, 0)), at(20, 9, 0));
        assert_eq!(daily.next_after(&at(20, 9, 0)), at(21, 9, 0), "strictly after");
        assert_eq!(daily.next_after(&at(20, 22, 0)), at(21, 9, 0));
    }

    #[test]
    fn weekdays_skip_the_weekend() {
        let weekdays = Schedule::parse("weekdays:09:00").unwrap();
        assert_eq!(weekdays.next_after(&at(20, 8, 0)), at(20, 9, 0), "Friday morning");
        assert_eq!(weekdays.next_after(&at(20, 10, 0)), at(23, 9, 0), "Friday evening goes to Monday");
        assert_eq!(weekdays.next_after(&at(22, 12, 0)), at(23, 9, 0), "Sunday goes to Monday");
    }

    #[test]
    fn a_mac_that_slept_through_several_slots_runs_once() {
        // Due at 09:00 Monday, woken on Thursday: the next run counts from Thursday.
        let daily = Schedule::parse("daily:09:00").unwrap();
        let woke = at(26, 14, 0);
        assert_eq!(daily.next_after(&woke), at(27, 9, 0));
    }

    #[test]
    fn stores_updates_and_removes_an_automation() {
        let db = Db::open_in_memory().unwrap();
        let created = db.create_automation(&input("daily:09:00")).unwrap();
        assert_eq!(created.name, "Morning news", "trimmed");
        assert_eq!(created.schedule, "daily:09:00");
        assert!(created.next_run_at.is_some());
        assert_eq!(db.list_automations().unwrap(), vec![created.clone()]);

        let off = AutomationInput { enabled: false, model: Some("qwen".to_string()), ..input("every:60") };
        let updated = db.update_automation(&created.id, &off).unwrap().unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.next_run_at, None, "nothing is due while switched off");
        assert_eq!(updated.model.as_deref(), Some("qwen"));
        assert!(db.update_automation("missing", &off).unwrap().is_none());

        assert!(db.delete_automation(&created.id).unwrap());
        assert!(!db.delete_automation(&created.id).unwrap());
        assert!(db.list_automations().unwrap().is_empty());
    }

    #[test]
    fn refuses_an_automation_that_cannot_run() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.create_automation(&AutomationInput { name: " ".to_string(), ..input("every:30") }).is_err());
        assert!(db.create_automation(&AutomationInput { prompt: "\n".to_string(), ..input("every:30") }).is_err());
        assert!(db.create_automation(&input("sometimes")).is_err());
        assert!(db.list_automations().unwrap().is_empty());
    }

    #[test]
    fn only_switched_on_automations_that_are_due_come_back() {
        let db = Db::open_in_memory().unwrap();
        let on = db.create_automation(&input("every:30")).unwrap();
        db.create_automation(&AutomationInput { enabled: false, ..input("every:30") }).unwrap();

        assert!(db.due_automations(Utc::now()).unwrap().is_empty(), "not due yet");
        let later = Utc::now() + Duration::hours(1);
        let due = db.due_automations(later).unwrap();
        assert_eq!(due.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec![on.id.as_str()]);
    }

    #[test]
    fn a_run_moves_the_next_one_on_and_clears_the_last_error() {
        let db = Db::open_in_memory().unwrap();
        let automation = db.create_automation(&input("every:30")).unwrap();
        let chat = db.create_conversation(None).unwrap();
        db.set_automation_error(&automation.id, Some("no model")).unwrap();

        let started = Local::now() + Duration::hours(2);
        db.record_automation_run(&automation.id, &chat.id, started).unwrap();

        let ran = db.get_automation(&automation.id).unwrap().unwrap();
        assert_eq!(ran.conversation_id.as_deref(), Some(chat.id.as_str()));
        assert_eq!(ran.last_run_at, Some(stamp(started)));
        assert_eq!(ran.next_run_at, Some(stamp(started + Duration::minutes(30))));
        assert_eq!(ran.last_error, None);
        assert!(db.due_automations((started + Duration::minutes(29)).with_timezone(&Utc)).unwrap().is_empty());

        // Deleting the chat leaves the automation, which makes a new chat next time.
        db.delete_conversation(&chat.id).unwrap();
        assert_eq!(db.get_automation(&automation.id).unwrap().unwrap().conversation_id, None);
    }
}
