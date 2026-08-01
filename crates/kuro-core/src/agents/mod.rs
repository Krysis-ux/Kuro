//! Specialists a coding turn can hand work to.
//!
//! A subagent is not a second model. It is the same model, given a different
//! brief, a different set of skills, and a fresh context containing only the task
//! it was handed — and that last part is the whole point.
//!
//! ## Why a fresh context is the feature
//!
//! The expensive failure in a long coding turn is not that the model is
//! insufficiently clever. It is that by the twentieth tool call its context holds
//! three files it no longer needs, a build log from a problem already fixed, and
//! a plan it has half-abandoned — and the answer degrades because of what is in
//! the window rather than what is missing from it.
//!
//! Delegating "work out why the login test fails" hands that question to a clean
//! context with the debugging and testing guidance loaded, and brings back a
//! paragraph instead of forty thousand tokens of investigation. The main turn
//! keeps its plan; the search happens somewhere else.
//!
//! ## Why the list is short
//!
//! There is one agent per *kind of work*, not one per language. A Rust agent, a
//! Kotlin agent and a Go agent would differ only in which language skill they
//! load — and that is already decided by what is in the project folder, from
//! [`crate::orchestrate::detect_languages`]. Adding twenty near-identical entries
//! would give a small model twenty things to choose between where the right
//! choice is almost always determined by the task rather than by the stack.
//!
//! So the split is by what the agent is *for*: finding things out, designing an
//! interface, building a backend, testing, reviewing, debugging. The language
//! comes from the project either way.

use serde::Serialize;

use crate::skills::{self, Skill};

/// One specialist.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Agent {
    pub slug: &'static str,
    pub name: &'static str,
    /// One line, for the picker and for the tool description.
    pub blurb: &'static str,
    /// When to reach for this one rather than another, written for the model
    /// making that choice.
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

pub fn find(slug: &str) -> Option<&'static Agent> {
    AGENTS
        .iter()
        .find(|agent| agent.slug.eq_ignore_ascii_case(slug))
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
    AGENTS.iter().map(|agent| agent.slug).collect()
}

/// The catalogue as one block of text, for the delegating model to choose from.
pub fn describe_for_model() -> String {
    let mut out = String::from("The specialists you can hand work to:\n");
    for agent in AGENTS {
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
        // A slug that was renamed would silently give an agent no expertise.
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
        // A reviewer that fixes what it finds has destroyed the review, and a
        // model given write tools will use them unless told otherwise.
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
        // Twenty near-identical language agents would give a small model twenty
        // things to choose between where the project folder already decides.
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
        for agent in AGENTS {
            assert!(described.contains(agent.slug));
        }
        assert_eq!(slugs().len(), AGENTS.len());
    }
}
