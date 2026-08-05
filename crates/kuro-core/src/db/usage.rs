//! What each provider's allowance has actually been spent on.
//!
//! Kuro records tokens per message and never added them up, so the free-models
//! screen could say which keys were stored and nothing at all about what they
//! had cost. This is the first aggregation in the database.
//!
//! ## What these numbers are, and are not
//!
//! They are a floor. Two things spend an allowance without appearing here:
//!
//! * A provider that sends no `usage` block at all — several free tiers do not
//!   — leaves both columns NULL, so the turn is counted but its tokens are not.
//!   That is why [`ProviderUsage`] carries `unreported_turns`: a number the user
//!   can see and judge beats a sentence hedging one they cannot.
//! * Subagent turns are never written as messages, so they are invisible here
//!   entirely.
//!
//! Forked conversations are excluded by construction rather than by a filter:
//! the fork copies a message's token counts, for the request inspector, but not
//! its `provider_slug` — and everything below keys on the slug being present.

use rusqlite::params;
use serde::Serialize;

use super::Db;
use crate::Result;

/// One provider's spend inside a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderUsage {
    #[serde(rename = "providerSlug")]
    pub provider_slug: String,
    /// Assistant turns sent to this provider, whether or not they reported.
    pub turns: i64,
    #[serde(rename = "promptTokens")]
    pub prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    pub completion_tokens: i64,
    /// Turns whose provider sent no counts, so the totals are a floor.
    #[serde(rename = "unreportedTurns")]
    pub unreported_turns: i64,
}

impl ProviderUsage {
    pub fn total_tokens(&self) -> i64 {
        self.prompt_tokens + self.completion_tokens
    }
}

impl Db {
    /// Tokens recorded against each provider in `[from, to)`.
    ///
    /// The bounds are RFC 3339 strings in the same shape `created_at` is
    /// written in, so this is a range scan over `idx_messages_usage` rather
    /// than a table scan. They are computed by the caller from the *user's* own
    /// clock: a day that rolled over at UTC midnight is somebody else's day, and
    /// for anyone west of Greenwich it would move an evening's work into
    /// tomorrow.
    pub fn usage_by_provider(&self, from: &str, to: &str) -> Result<Vec<ProviderUsage>> {
        self.with(|conn| {
            let mut statement = conn.prepare(
                "SELECT provider_slug,
                        COUNT(*) AS turns,
                        COALESCE(SUM(COALESCE(usage_prompt_tokens_total,
                                              usage_prompt_tokens)), 0) AS prompt_tokens,
                        COALESCE(SUM(usage_completion_tokens), 0)       AS completion_tokens,
                        SUM(CASE WHEN usage_completion_tokens IS NULL
                                 THEN 1 ELSE 0 END)                     AS unreported
                   FROM messages
                  WHERE provider_slug IS NOT NULL
                    AND created_at >= ?1
                    AND created_at <  ?2
                  GROUP BY provider_slug
                  ORDER BY completion_tokens DESC",
            )?;

            let rows = statement.query_map(params![from, to], |row| {
                Ok(ProviderUsage {
                    provider_slug: row.get("provider_slug")?,
                    turns: row.get("turns")?,
                    // `COALESCE` on the sum keeps these non-null even when
                    // every row in the group reported nothing.
                    prompt_tokens: row.get("prompt_tokens")?,
                    completion_tokens: row.get("completion_tokens")?,
                    unreported_turns: row.get("unreported")?,
                })
            })?;

            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{MessageCompletion, NewMessage};

    /// A conversation with one assistant turn attributed to `slug`.
    fn turn(db: &Db, conversation: &str, slug: &str, prompt: i64, completion: i64) -> String {
        let message = db
            .insert_message(
                conversation,
                &NewMessage {
                    role: "assistant".to_string(),
                    provider_slug: Some(slug.to_string()),
                    ..Default::default()
                },
            )
            .expect("insert");

        db.complete_message(
            &message.id,
            &MessageCompletion {
                content: "hello".to_string(),
                usage_prompt_tokens: Some(prompt),
                usage_prompt_tokens_total: Some(prompt),
                usage_completion_tokens: Some(completion),
                ..Default::default()
            },
        )
        .expect("complete");

        message.id
    }

    fn conversation(db: &Db) -> String {
        db.create_conversation(None).expect("conversation").id
    }

    /// A window wide enough to hold anything written during a test.
    const ALWAYS: (&str, &str) = ("2000-01-01T00:00:00Z", "2999-01-01T00:00:00Z");

    #[test]
    fn tokens_are_summed_per_provider() {
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);

        turn(&db, &chat, "groq", 100, 20);
        turn(&db, &chat, "groq", 50, 10);
        turn(&db, &chat, "cerebras", 7, 3);

        let usage = db.usage_by_provider(ALWAYS.0, ALWAYS.1).expect("usage");

        let groq = usage.iter().find(|row| row.provider_slug == "groq").expect("groq");
        assert_eq!(groq.turns, 2);
        assert_eq!(groq.prompt_tokens, 150);
        assert_eq!(groq.completion_tokens, 30);
        assert_eq!(groq.total_tokens(), 180);

        let cerebras = usage.iter().find(|row| row.provider_slug == "cerebras").expect("row");
        assert_eq!(cerebras.total_tokens(), 10);
    }

    #[test]
    fn a_local_turn_spends_no_allowance_and_is_not_counted() {
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);

        db.insert_message(&chat, &NewMessage::assistant("local reply")).expect("insert");

        assert!(db.usage_by_provider(ALWAYS.0, ALWAYS.1).expect("usage").is_empty());
    }

    #[test]
    fn a_provider_that_reports_nothing_is_counted_as_a_turn_and_flagged() {
        // The honest gap: several free tiers send no usage block, so the tokens
        // are unknowable and the screen has to say how many turns that covers
        // rather than quietly reporting a smaller number.
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);

        let message = db
            .insert_message(
                &chat,
                &NewMessage {
                    role: "assistant".to_string(),
                    provider_slug: Some("kilo".to_string()),
                    ..Default::default()
                },
            )
            .expect("insert");
        db.complete_message(
            &message.id,
            &MessageCompletion {
                content: "no counts came back".to_string(),
                ..Default::default()
            },
        )
        .expect("complete");

        let usage = db.usage_by_provider(ALWAYS.0, ALWAYS.1).expect("usage");
        let kilo = &usage[0];

        assert_eq!(kilo.turns, 1);
        assert_eq!(kilo.unreported_turns, 1);
        assert_eq!(kilo.total_tokens(), 0, "unknown must not be reported as zero-cost");
    }

    #[test]
    fn the_summed_prompt_count_wins_over_the_last_rounds() {
        // A five-round agentic turn sent five prompts and was charged for five.
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);

        let message = db
            .insert_message(
                &chat,
                &NewMessage {
                    role: "assistant".to_string(),
                    provider_slug: Some("groq".to_string()),
                    ..Default::default()
                },
            )
            .expect("insert");
        db.complete_message(
            &message.id,
            &MessageCompletion {
                content: "done".to_string(),
                usage_prompt_tokens: Some(900),
                usage_prompt_tokens_total: Some(4_500),
                usage_completion_tokens: Some(100),
                ..Default::default()
            },
        )
        .expect("complete");

        let usage = db.usage_by_provider(ALWAYS.0, ALWAYS.1).expect("usage");

        assert_eq!(usage[0].prompt_tokens, 4_500, "the last round alone understates the spend");
    }

    #[test]
    fn a_row_written_before_the_upgrade_still_contributes_what_it_knows() {
        // `usage_prompt_tokens_total` is NULL on every row that existed before
        // this column did. Those turns should count their last-round figure
        // rather than nothing at all.
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);

        let message = db
            .insert_message(
                &chat,
                &NewMessage {
                    role: "assistant".to_string(),
                    provider_slug: Some("groq".to_string()),
                    ..Default::default()
                },
            )
            .expect("insert");
        db.complete_message(
            &message.id,
            &MessageCompletion {
                content: "done".to_string(),
                usage_prompt_tokens: Some(300),
                usage_prompt_tokens_total: None,
                usage_completion_tokens: Some(40),
                ..Default::default()
            },
        )
        .expect("complete");

        assert_eq!(db.usage_by_provider(ALWAYS.0, ALWAYS.1).expect("usage")[0].prompt_tokens, 300);
    }

    #[test]
    fn a_window_bound_must_be_utc_to_compare_correctly() {
        // Caught in testing. `created_at` is written in UTC and the comparison
        // is a string one, so a bound printed with a local offset compares by
        // its digits rather than by the instant it names. On a machine four
        // hours behind UTC, an evening turn is written as tomorrow's date and
        // fell outside a "today" whose upper bound still read as today.
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);
        turn(&db, &chat, "groq", 100, 20);

        let now_utc = chrono::Utc::now();
        let wide = db
            .usage_by_provider("2000-01-01T00:00:00+00:00", &now_utc.to_rfc3339())
            .expect("usage");
        assert_eq!(wide.len(), 1, "a UTC upper bound must include a turn just written");

        // The same instant, printed with a western offset, sorts earlier as a
        // string even though it is the same moment.
        let local_shaped = now_utc
            .with_timezone(&chrono::FixedOffset::west_opt(4 * 3600).expect("offset"))
            .to_rfc3339();
        assert!(
            local_shaped < now_utc.to_rfc3339(),
            "this is the trap the production code must avoid: {local_shaped} < {}",
            now_utc.to_rfc3339()
        );
    }

    #[test]
    fn the_window_excludes_what_falls_outside_it() {
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);
        turn(&db, &chat, "groq", 100, 20);

        // A window that ends before anything was written.
        let empty = db
            .usage_by_provider("2000-01-01T00:00:00Z", "2000-01-02T00:00:00Z")
            .expect("usage");

        assert!(empty.is_empty());
    }

    #[test]
    fn forking_a_conversation_does_not_spend_the_allowance_again() {
        // The double-count this guards against: a fork copies the token numbers
        // so the inspector still shows them, but not the attribution, because
        // nothing was sent to a provider.
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);
        let message = turn(&db, &chat, "groq", 100, 20);

        let before = db.usage_by_provider(ALWAYS.0, ALWAYS.1).expect("usage")[0].total_tokens();
        db.fork_conversation(&chat, Some(&message)).expect("fork");
        let after = db.usage_by_provider(ALWAYS.0, ALWAYS.1).expect("usage");

        assert_eq!(after.len(), 1, "a fork must not add a provider");
        assert_eq!(after[0].total_tokens(), before, "a fork spends nothing");
    }
}
