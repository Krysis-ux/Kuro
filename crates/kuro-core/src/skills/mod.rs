//! Skills: expertise you switch on.
//!
//! A skill is a short, specific instruction block appended to the system prompt.
//! That is all. There is no execution, no sandbox, no plugin runtime — and that
//! restraint is the point, because it is what makes a skill safe to install with
//! one click and impossible to break the application with.
//!
//! The reason this is worth having at all is that prompt guidance is the highest
//! leverage available on a small local model. A 4B model asked for Rust will
//! cheerfully write code that does not compile; the same model told "every snippet
//! must include its `use` statements, prefer `?` over `unwrap`, and say which
//! edition you assume" produces materially better output. That is a real gain for
//! zero inference cost, and it is exactly the gain a hosted frontier model gets
//! from a system prompt somebody spent a month on.
//!
//! Skills are deliberately not auto-selected. Guessing which expertise a message
//! needs is a classification problem that would be wrong often enough to be
//! annoying, and being wrong here means silently changing how the model answers.

use serde::Serialize;

use crate::db::Db;
use crate::Result;

/// Settings key holding the slugs the user switched on.
pub const KEY_ENABLED: &str = "skills.enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillCategory {
    Language,
    Practice,
    Writing,
}

impl SkillCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Language => "Languages",
            Self::Practice => "Engineering practice",
            Self::Writing => "Writing and reasoning",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub slug: &'static str,
    pub name: &'static str,
    /// One line, for the card.
    pub blurb: &'static str,
    pub category: SkillCategory,
    /// What gets appended to the system prompt. Written as imperatives, because
    /// that is what a small model follows most reliably.
    pub instructions: &'static str,
    /// Roughly how much context this costs, so the UI can warn when several are on.
    pub approx_tokens: usize,
}

/// The catalogue.
///
/// Every entry is guidance a competent reviewer of that language would actually
/// give. Nothing here is filler: a skill that says "write good code" would cost
/// context and change nothing.
pub const SKILLS: &[Skill] = &[
    Skill {
        slug: "rust",
        name: "Rust",
        blurb: "Compiling code, real error handling, no stray unwraps.",
        category: SkillCategory::Language,
        approx_tokens: 130,
        instructions: "\
When writing Rust:
- Include every `use` statement the snippet needs. A snippet that does not compile is not an answer.
- Return `Result<T, E>` and use `?`. Reserve `unwrap`/`expect` for tests and genuinely unreachable states, and say which it is.
- Borrow (`&str`, `&[T]`) in parameters; take ownership only when you store or consume the value.
- Never clone to silence the borrow checker without saying why the clone is correct.
- Say which edition you assume when it matters (2021 unless told otherwise).
- Prefer iterator chains for transformation, loops for control flow with early exits.
- For async, name the runtime (`tokio` unless told otherwise) and do not block inside an async fn.",
    },
    Skill {
        slug: "python",
        name: "Python",
        blurb: "Type hints, real error handling, stdlib before dependencies.",
        category: SkillCategory::Language,
        approx_tokens: 120,
        instructions: "\
When writing Python:
- Target 3.11+ unless told otherwise. Use `list[str]`, `dict[str, int]`, `X | None` — not `typing.List` or `Optional`.
- Add type hints to every function signature you write.
- Catch specific exceptions, never bare `except:`. Never swallow an exception silently.
- Use `pathlib` over `os.path`, f-strings over `%` and `.format`.
- Prefer the standard library. Name any third-party dependency explicitly and say why it is worth adding.
- Use a `if __name__ == \"__main__\":` guard for anything runnable.
- Never mutate a default argument; use `None` and build inside the function.",
    },
    Skill {
        slug: "typescript",
        name: "TypeScript",
        blurb: "No `any`, narrow unknown, correct async.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        instructions: "\
When writing TypeScript:
- Never use `any`. Use `unknown` for untrusted input and narrow it before use.
- Type every exported function's parameters and return value. Let local inference do the rest.
- Use `type` for unions and `interface` for object shapes that may be extended.
- Prefer string literal unions over `enum`.
- `await` every promise or handle it explicitly; never leave a floating promise.
- Update state immutably — spread rather than assign into an existing object.
- Say which runtime you assume (Node, browser, Deno) when it changes the answer.",
    },
    Skill {
        slug: "go",
        name: "Go",
        blurb: "Idiomatic error wrapping, no goroutine leaks.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        instructions: "\
When writing Go:
- Handle every error. Wrap with context: `fmt.Errorf(\"doing X: %w\", err)`.
- Never use `panic` for ordinary failure, and never ignore an error with `_` without saying why.
- Accept interfaces, return concrete types.
- Pass `context.Context` as the first parameter to anything that does I/O, and honour cancellation.
- Every goroutine you start must have a stated way to stop. Use `defer` for cleanup.
- Guard shared state with a mutex or a channel, and say which and why.
- Keep to the standard library unless a dependency is clearly justified.",
    },
    Skill {
        slug: "sql",
        name: "SQL",
        blurb: "Parameterised queries, indexes, no accidental table scans.",
        category: SkillCategory::Language,
        approx_tokens: 100,
        instructions: "\
When writing SQL:
- Always use bound parameters. Never concatenate a value into a query string, not even in an example.
- Name the dialect you are writing for (PostgreSQL unless told otherwise) — the syntax differs.
- Say which index a query relies on, and flag any query that would scan a whole table.
- Prefer explicit `JOIN ... ON` over `WHERE` joins, and never `SELECT *` in application code.
- Add `LIMIT` to anything that could return an unbounded result set.
- For a migration, give the rollback too, and flag any statement that locks a table.",
    },
    Skill {
        slug: "shell",
        name: "Shell",
        blurb: "Safe scripts that fail loudly instead of half-working.",
        category: SkillCategory::Language,
        approx_tokens: 90,
        instructions: "\
When writing shell scripts:
- Start with `set -euo pipefail`. A script that continues after a failed step is a hazard.
- Quote every variable expansion: `\"$var\"`, `\"${array[@]}\"`.
- Use `$(...)` not backticks, and `[[ ]]` not `[ ]` in bash.
- Never parse `ls`. Use globs or `find -print0` with `read -d ''`.
- Before any destructive command, say what it deletes. Never suggest `rm -rf` with a variable path unquoted.
- Say which shell you assume, and prefer POSIX `sh` when portability matters.",
    },
    Skill {
        slug: "code-review",
        name: "Code review",
        blurb: "Severity-ordered findings with the fix, not vibes.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
        instructions: "\
When reviewing code:
- Order findings by severity: correctness and security first, then maintainability, then style.
- For each finding give the location, what breaks, and the concrete fix. No finding without a fix.
- Describe a failure as inputs and the wrong result it produces, not as a feeling about the code.
- Check specifically: unhandled errors, off-by-one and boundary cases, unvalidated input, concurrency on shared state, resources never released.
- Say what is already correct, briefly, so the review is usable rather than demoralising.
- If you cannot see enough of the code to judge, say which part you need.",
    },
    Skill {
        slug: "debugging",
        name: "Debugging",
        blurb: "Form a hypothesis, then test it — no shotgun fixes.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
        instructions: "\
When debugging:
- Restate the symptom precisely: what was expected, what happened, and how reproducibly.
- Give the most likely cause first with your reasoning, then the next two, ranked.
- For each cause, state the one check that would confirm or eliminate it. Prefer checks that split the search space.
- Do not suggest changing several things at once; a fix that works for unknown reasons is not a fix.
- Ask for the exact error text, stack trace or log line when you do not have it. Do not guess at what it says.
- Once the cause is known, say why it produced this symptom before giving the patch.",
    },
    Skill {
        slug: "explaining",
        name: "Explaining",
        blurb: "Plain answers first, detail after, no lecture.",
        category: SkillCategory::Writing,
        approx_tokens: 90,
        instructions: "\
When explaining something:
- Answer in the first sentence. Put the conclusion before the reasoning.
- Use the shortest accurate wording. Cut restatements of the question, and cut \"great question\".
- Define a term the first time you use it, in a clause, not a paragraph.
- Give one concrete example rather than three abstract ones.
- Match the depth to the question: a one-line question gets a one-line answer.
- Say what you are simplifying when the simplification would mislead if taken literally.",
    },
    Skill {
        slug: "step-by-step",
        name: "Working carefully",
        blurb: "Think before answering. Helps small models most.",
        category: SkillCategory::Writing,
        approx_tokens: 100,
        instructions: "\
For any question involving arithmetic, logic, dates, counting, or several constraints at once:
- Work it through in short numbered steps before giving the answer.
- Do arithmetic one operation at a time. Do not skip to the result.
- When counting, enumerate the items and then count them.
- After reaching an answer, check it against the original question and each stated constraint.
- If a check fails, say so and redo it rather than defending the first answer.",
    },
];

pub fn find(slug: &str) -> Option<&'static Skill> {
    SKILLS.iter().find(|skill| skill.slug == slug)
}

/// Slugs the user has switched on.
pub fn enabled_slugs(db: &Db) -> Result<Vec<String>> {
    let Some(stored) = db.get_setting(KEY_ENABLED)? else {
        return Ok(Vec::new());
    };

    Ok(stored
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                // A slug from a newer build that this one does not have is dropped
                // rather than failing the request.
                .filter(|slug| find(slug).is_some())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}

/// The skills to inject, resolved from storage.
pub fn enabled(db: &Db) -> Result<Vec<&'static Skill>> {
    Ok(enabled_slugs(db)?
        .iter()
        .filter_map(|slug| find(slug))
        .collect())
}

pub fn set_enabled(db: &Db, slugs: &[String]) -> Result<Vec<String>> {
    let kept: Vec<&str> = slugs
        .iter()
        .filter(|slug| find(slug).is_some())
        .map(String::as_str)
        .collect();

    db.set_setting(KEY_ENABLED, &serde_json::json!(kept))?;
    Ok(kept.into_iter().map(str::to_string).collect())
}

/// Rough context cost of a set of skills, for the warning in the UI.
pub fn approx_tokens(skills: &[&Skill]) -> usize {
    skills.iter().map(|skill| skill.approx_tokens).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slug_is_unique_and_findable() {
        let mut seen: Vec<&str> = Vec::new();
        for skill in SKILLS {
            assert!(!seen.contains(&skill.slug), "duplicate slug `{}`", skill.slug);
            seen.push(skill.slug);
            assert!(find(skill.slug).is_some());
        }
        assert!(find("cobol").is_none());
    }

    #[test]
    fn every_skill_gives_specific_instructions_not_platitudes() {
        for skill in SKILLS {
            assert!(
                skill.instructions.lines().count() >= 5,
                "`{}` should give several concrete rules",
                skill.slug
            );
            assert!(
                skill.instructions.contains('-'),
                "`{}` should be a list of imperatives",
                skill.slug
            );
            assert!(!skill.blurb.is_empty());
            assert!(skill.approx_tokens > 0);
        }
    }

    #[test]
    fn instructions_stay_small_enough_to_combine() {
        for skill in SKILLS {
            let words = skill.instructions.split_whitespace().count();
            assert!(
                words < 220,
                "`{}` is {words} words; several of these have to fit at once",
                skill.slug
            );
        }
    }

    #[test]
    fn the_catalogue_covers_the_languages_people_ask_for() {
        for expected in ["rust", "python", "typescript", "go", "sql"] {
            let skill = find(expected).unwrap_or_else(|| panic!("missing `{expected}`"));
            assert_eq!(skill.category, SkillCategory::Language);
        }
    }

    #[test]
    fn nothing_is_enabled_on_a_fresh_install() {
        let db = Db::open_in_memory().expect("open");
        assert!(enabled_slugs(&db).expect("slugs").is_empty());
        assert!(enabled(&db).expect("skills").is_empty());
    }

    #[test]
    fn enabling_round_trips_through_storage() {
        let db = Db::open_in_memory().expect("open");

        let kept = set_enabled(&db, &["rust".to_string(), "debugging".to_string()]).expect("set");

        assert_eq!(kept.len(), 2);
        let resolved = enabled(&db).expect("enabled");
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().any(|skill| skill.slug == "rust"));
    }

    #[test]
    fn an_unknown_slug_is_dropped_rather_than_stored() {
        let db = Db::open_in_memory().expect("open");

        let kept = set_enabled(&db, &["rust".to_string(), "not-a-skill".to_string()]).expect("set");

        assert_eq!(kept, vec!["rust".to_string()]);
        assert_eq!(enabled_slugs(&db).expect("slugs"), vec!["rust".to_string()]);
    }

    #[test]
    fn a_slug_from_a_newer_build_is_ignored_on_read() {
        let db = Db::open_in_memory().expect("open");
        db.set_setting(KEY_ENABLED, &serde_json::json!(["rust", "quantum-basic"]))
            .expect("set");

        assert_eq!(enabled_slugs(&db).expect("slugs"), vec!["rust".to_string()]);
    }

    #[test]
    fn enabling_nothing_clears_the_selection() {
        let db = Db::open_in_memory().expect("open");
        set_enabled(&db, &["rust".to_string()]).expect("set");

        set_enabled(&db, &[]).expect("clear");

        assert!(enabled(&db).expect("enabled").is_empty());
    }

    #[test]
    fn context_cost_is_the_sum_of_what_is_on() {
        let rust = find("rust").expect("rust");
        let go = find("go").expect("go");
        assert_eq!(
            approx_tokens(&[rust, go]),
            rust.approx_tokens + go.approx_tokens
        );
        assert_eq!(approx_tokens(&[]), 0);
    }

    #[test]
    fn every_category_has_a_label_for_the_store() {
        for category in [
            SkillCategory::Language,
            SkillCategory::Practice,
            SkillCategory::Writing,
        ] {
            assert!(!category.label().is_empty());
        }
    }
}
