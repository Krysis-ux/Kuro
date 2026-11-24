//! Specialists a coding turn can hand work to.
//!
//! A subagent is the same model with a different brief, different skills and a
//! fresh context holding only the task it was handed.
//!
//! The fresh context is the point. By the twentieth tool call a long turn's
//! context is full of stale files and build logs, and answers degrade because of
//! what is in the window. Delegating "work out why the login test fails" hands
//! that to a clean context and brings back a paragraph instead of forty thousand
//! tokens of investigation.
//!
//! The list is short: one agent per *kind of work*, not per language. The
//! language skill is already chosen from the project folder
//! ([`crate::orchestrate::detect_languages`]), and twenty near-identical agents
//! would only give a small model more to choose between.

use std::sync::RwLock;

use serde::Serialize;

use crate::db::{Db, UserAgentRecord};
use crate::skills::{self, Skill};
use crate::{KuroError, Result};

/// One specialist.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Agent {
    pub slug: &'static str,
    pub name: &'static str,
    /// One line, for the picker and for the tool description.
    pub blurb: &'static str,
    /// When to reach for this one rather than another, written for the delegating
    /// model.
    pub when: &'static str,
    /// What the agent is told it is. Prepended to the ordinary workspace brief.
    pub brief: &'static str,
    /// Skills loaded on top of whatever the project justifies.
    pub skills: &'static [&'static str],
    /// Whether it may change files. A reviewer that edits is not a reviewer.
    pub writes: bool,
}

pub const AGENTS: &[Agent] = &[
    Agent {
        slug: "explore",
        name: "Explorer",
        blurb: "Reads the project and reports how something works.",
        when: "Use when you need to understand code before changing it, and the answer \
               would cost several reads to find.",
        brief: "You are an explorer. Your job is to find out how something works and report \
                it — not to change anything. Read widely, follow the imports, and answer with \
                specific file paths and line numbers rather than generalities. Finish with a \
                short summary that someone who has not read the code could act on.",
        skills: &["codebase-navigation"],
        writes: false,
    },
    Agent {
        slug: "frontend",
        name: "Interface builder",
        blurb: "Builds and changes user interface code.",
        when: "Use for anything a person looks at: components, layout, styling, states.",
        brief: "You are building an interface. Match the project's existing components, tokens \
                and conventions rather than introducing your own. Write every state — empty, \
                loading, error, one item, far too many — and check the result in the browser \
                if you can run one.",
        skills: &["frontend-craft", "component-design", "ui-design", "accessibility"],
        writes: true,
    },
    Agent {
        slug: "backend",
        name: "Backend builder",
        blurb: "Builds services, APIs and data access.",
        when: "Use for handlers, services, schemas, migrations and anything that talks to a \
               database or another service.",
        brief: "You are building server-side code. Validate at the boundary, keep handlers \
                thin, parameterise every query, and bound everything that could grow without \
                limit. Say what happens when each dependency is slow or down.",
        skills: &["backend-services", "data-modelling", "api-design", "error-handling"],
        writes: true,
    },
    Agent {
        slug: "debug",
        name: "Debugger",
        blurb: "Finds the cause of a specific failure.",
        when: "Use when something is broken and you do not yet know why. Give it the exact \
               symptom and the exact error text.",
        brief: "You are debugging. Restate the symptom precisely, form the most likely \
                hypothesis, and test it — one change at a time. Run the failing thing yourself \
                rather than reasoning about it. Report the cause and why it produced this \
                symptom before giving the fix.",
        skills: &["debugging", "reading-errors", "running-code"],
        writes: true,
    },
    Agent {
        slug: "test",
        name: "Tester",
        blurb: "Writes and runs tests.",
        when: "Use to cover a change you have just made, or to characterise behaviour before \
               refactoring it.",
        brief: "You are writing tests. Name each one after the behaviour it protects, give it \
                one reason to fail, and cover the boundaries — empty, one, many, malformed. \
                Run them. A test you have not seen fail for the right reason is not evidence.",
        skills: &["testing", "running-code", "verification"],
        writes: true,
    },
    Agent {
        slug: "review",
        name: "Reviewer",
        blurb: "Reviews a change and reports what is wrong with it.",
        when: "Use after a change is made, before saying it is finished.",
        brief: "You are reviewing, not fixing. Read what changed and report findings ordered \
                by severity: correctness and security first, then maintainability, then style. \
                Give the location, what breaks, and the concrete fix for each. Say what is \
                already correct, briefly. Do not edit any file.",
        skills: &["code-review", "security", "verification"],
        writes: false,
    },
    Agent {
        slug: "architect",
        name: "Architect",
        blurb: "Designs how something should be built, before it is.",
        when: "Use for a change that spans several files or introduces a new concept, when the \
               shape is not yet obvious.",
        brief: "You are designing rather than building. Start from the constraints and what \
                already exists in this project. Give the simplest design that meets them, name \
                the trade-off against one real alternative, and say what would have to change \
                for it to stop working. Do not edit any file — return the plan.",
        skills: &["architecture", "codebase-navigation"],
        writes: false,
    },
    Agent {
        slug: "refactor",
        name: "Refactorer",
        blurb: "Improves the shape of code without changing what it does.",
        when: "Use when code works and is hard to work with. Not for behaviour changes.",
        brief: "You are refactoring. Behaviour must not change. Establish first how it is \
                currently verified, then change one thing at a time and re-run that check after \
                each step. Name the smell you are removing. Never mix a refactor with a fix.",
        skills: &["refactoring", "running-code", "verification"],
        writes: true,
    },
];

/// Subagents the user wrote, leaked into `'static` when loaded so they share
/// `Agent` with the built-ins. The same trade as `skills::custom`: a few hundred
/// bytes per agent, leaked only when the user saves or removes one.
static CUSTOM: RwLock<Vec<&'static Agent>> = RwLock::new(Vec::new());

/// Skills one user agent may load. More would crowd its brief.
pub const MAX_CUSTOM_SKILLS: usize = 8;
const MAX_NAME_CHARS: usize = 40;
const MAX_BLURB_CHARS: usize = 160;
const MAX_WHEN_CHARS: usize = 400;
const MAX_BRIEF_CHARS: usize = 6_000;

/// Every agent a turn can delegate to: the built-ins, then the user's.
pub fn all() -> Vec<&'static Agent> {
    let custom = CUSTOM.read().expect("custom agents lock");
    AGENTS.iter().chain(custom.iter().copied()).collect()
}

/// Whether a slug belongs to Kuro rather than to the user (built-ins cannot be
/// edited or removed).
pub fn is_builtin(slug: &str) -> bool {
    AGENTS
        .iter()
        .any(|agent| agent.slug.eq_ignore_ascii_case(slug))
}

pub fn find(slug: &str) -> Option<&'static Agent> {
    all()
        .into_iter()
        .find(|agent| agent.slug.eq_ignore_ascii_case(slug))
}

/// Load the user's agents from storage, replacing the whole list. Called at
/// startup and after anything that changes the table.
pub fn reload(db: &Db) -> Result<usize> {
    let loaded: Vec<&'static Agent> = db.list_user_agents()?.iter().map(leak).collect();
    let count = loaded.len();
    *CUSTOM.write().expect("custom agents lock") = loaded;
    Ok(count)
}

fn leak(record: &UserAgentRecord) -> &'static Agent {
    let text = |value: &str| -> &'static str { Box::leak(value.to_owned().into_boxed_str()) };
    let skills: Vec<&'static str> = record.skills.iter().map(|slug| text(slug)).collect();
    Box::leak(Box::new(Agent {
        slug: text(&record.slug),
        name: text(&record.name),
        blurb: text(&record.blurb),
        when: text(&record.when),
        brief: text(&record.brief),
        skills: Box::leak(skills.into_boxed_slice()),
        writes: record.writes,
    }))
}

/// Refuse a user agent that would not work, or would crowd the list the
/// delegating model chooses from.
pub fn check(record: &UserAgentRecord) -> Result<()> {
    let field = |label: &str, value: &str, max: usize, required: bool| -> Result<()> {
        if required && value.trim().is_empty() {
            return Err(KuroError::bad_request(format!("{label} is required.")));
        }
        if value.chars().count() > max {
            return Err(KuroError::bad_request(format!(
                "{label} is too long. Keep it under {max} characters."
            )));
        }
        Ok(())
    };

    field("A name", &record.name, MAX_NAME_CHARS, true)?;
    if record.slug.is_empty() {
        return Err(KuroError::bad_request(
            "The name needs at least one letter or digit.",
        ));
    }
    field("The description", &record.blurb, MAX_BLURB_CHARS, false)?;
    field("When to use it", &record.when, MAX_WHEN_CHARS, false)?;
    field("Instructions", &record.brief, MAX_BRIEF_CHARS, true)?;

    if record.skills.len() > MAX_CUSTOM_SKILLS {
        return Err(KuroError::bad_request(format!(
            "Pick at most {MAX_CUSTOM_SKILLS} skills."
        )));
    }
    if let Some(missing) = record.skills.iter().find(|slug| skills::find(slug).is_none()) {
        return Err(KuroError::bad_request(format!(
            "There is no skill called `{missing}`."
        )));
    }
    Ok(())
}

impl Agent {
    /// The skills this agent loads, dropping any slug that no longer exists.
    pub fn resolved_skills(&self) -> Vec<&'static Skill> {
        self.skills
            .iter()
            .filter_map(|slug| skills::find(slug))
            .collect()
    }
}

/// The names, for the tool schema's enum.
pub fn slugs() -> Vec<&'static str> {
    all().iter().map(|agent| agent.slug).collect()
}

/// The catalogue as one block of text, for the delegating model to choose from.
pub fn describe_for_model() -> String {
    let mut out = String::from("The specialists you can hand work to:\n");
    for agent in all() {
        out.push_str(&format!(
            "- `{}` — {} {}\n",
            agent.slug, agent.blurb, agent.when
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slug_is_unique_and_findable() {
        let mut seen: Vec<&str> = Vec::new();
        for agent in AGENTS {
            assert!(!seen.contains(&agent.slug), "duplicate `{}`", agent.slug);
            seen.push(agent.slug);
            assert!(find(agent.slug).is_some());
        }
        assert!(find("not-an-agent").is_none());
        assert_eq!(find("EXPLORE").map(|agent| agent.slug), Some("explore"));
    }

    #[test]
    fn every_agent_says_what_it_is_for_and_when_to_use_it() {
        for agent in AGENTS {
            assert!(agent.blurb.len() > 20, "`{}` needs a real blurb", agent.slug);
            assert!(agent.when.len() > 30, "`{}` needs to say when to use it", agent.slug);
            assert!(agent.brief.len() > 100, "`{}` needs a real brief", agent.slug);
        }
    }

    #[test]
    fn every_agents_skills_exist() {
        // A renamed slug would silently give an agent no expertise.
        for agent in AGENTS {
            assert_eq!(
                agent.resolved_skills().len(),
                agent.skills.len(),
                "`{}` names a skill that does not exist: {:?}",
                agent.slug,
                agent.skills
            );
        }
    }

    #[test]
    fn the_agents_that_only_report_are_told_not_to_edit() {
        // A reviewer that fixes what it finds destroys the review, and a model given
        // write tools will use them.
        for agent in AGENTS.iter().filter(|agent| !agent.writes) {
            assert!(
                agent.brief.contains("not to change")
                    || agent.brief.contains("Do not edit")
                    || agent.brief.contains("not fixing"),
                "`{}` cannot write, so its brief must say so",
                agent.slug
            );
        }
    }

    #[test]
    fn there_is_an_agent_for_each_kind_of_work_rather_than_each_language() {
        // Near-identical language agents would only add choices; the project folder
        // already decides the language.
        for expected in ["explore", "frontend", "backend", "debug", "test", "review"] {
            assert!(find(expected).is_some(), "missing `{expected}`");
        }
        assert!(
            AGENTS.len() <= 10,
            "the list has to stay choosable; got {}",
            AGENTS.len()
        );
        for agent in AGENTS {
            assert!(
                skills::find(agent.slug).is_none(),
                "`{}` reads as a language rather than a kind of work",
                agent.slug
            );
        }
    }

    #[test]
    fn the_catalogue_reads_as_a_choice_the_model_can_make() {
        let described = describe_for_model();
        let listed = slugs();
        for agent in AGENTS {
            assert!(described.contains(agent.slug));
            assert!(listed.contains(&agent.slug));
        }
    }

    fn user_agent(name: &str) -> UserAgentRecord {
        UserAgentRecord {
            slug: skills::custom::slugify(name),
            name: name.to_string(),
            blurb: "Writes the docs.".to_string(),
            when: "Use for READMEs and guides.".to_string(),
            brief: "You write documentation.".to_string(),
            skills: vec!["testing".to_string()],
            writes: false,
        }
    }

    #[test]
    fn a_user_agent_can_be_delegated_to_once_loaded() {
        let db = Db::open_in_memory().unwrap();
        let record = user_agent("Docs Writer Zeta");
        check(&record).unwrap();
        db.put_user_agent(&record).unwrap();

        assert_eq!(reload(&db).unwrap(), 1);
        let found = find("docs-writer-zeta").expect("loaded agent is findable");
        assert_eq!(found.name, "Docs Writer Zeta");
        assert!(!found.writes);
        assert_eq!(found.resolved_skills().len(), 1);
        assert!(!is_builtin("docs-writer-zeta"));
        assert!(describe_for_model().contains("docs-writer-zeta"));

        // Removing it from storage and reloading takes it away again.
        db.delete_user_agent("docs-writer-zeta").unwrap();
        reload(&db).unwrap();
        assert!(find("docs-writer-zeta").is_none());
    }

    #[test]
    fn a_user_agent_that_would_not_work_is_refused() {
        assert!(check(&UserAgentRecord { name: "   ".to_string(), ..user_agent("x") }).is_err());
        assert!(check(&UserAgentRecord { slug: String::new(), ..user_agent("!!!") }).is_err());
        assert!(check(&UserAgentRecord { brief: String::new(), ..user_agent("Docs") }).is_err());
        assert!(check(&UserAgentRecord { brief: "x".repeat(6_001), ..user_agent("Docs") }).is_err());
        assert!(check(&UserAgentRecord {
            skills: vec!["no-such-skill".to_string()],
            ..user_agent("Docs")
        })
        .is_err());
        assert!(check(&UserAgentRecord {
            skills: vec!["testing".to_string(); MAX_CUSTOM_SKILLS + 1],
            ..user_agent("Docs")
        })
        .is_err());
        assert!(check(&user_agent("Docs")).is_ok());
    }
}
