//! Turning an effort level into an actual amount of work.
//!
//! The effort control used to change two numbers — a token budget and a
//! temperature — which is the least interesting thing it could change. On a
//! coding turn what "try harder" should mean is closer to: read more of the
//! project before answering, be willing to run the tests twice, and bring the
//! expertise that matches what this project is written in.
//!
//! So effort is resolved into a [`Plan`]: how many tool rounds the turn may
//! spend, and which skills go into the brief on top of whatever the user
//! switched on.
//!
//! ## Why auto-selection is defensible here and was not before
//!
//! [`crate::skills`] says outright that skills are not auto-selected, because
//! guessing which expertise a *message* needs is a classification problem that
//! is wrong often enough to be annoying. That still holds for chat, and chat
//! still gets nothing but the essentials.
//!
//! A coding workspace is a different problem. It is not a guess: a folder with a
//! `Cargo.toml` in it is a Rust project, and there is no interpretation under
//! which the Rust skill is the wrong thing to bring. The evidence is a file on
//! disk rather than an inference about intent, which is why this is allowed to
//! be automatic and why it only ever adds language skills for languages the
//! project demonstrably contains.
//!
//! It is also switchable, per surface, because somebody who has curated their
//! own skill list should not have it silently added to.

use std::path::Path;

use serde::Serialize;

use crate::settings::Effort;
use crate::skills::{self, Skill};
use crate::workspace::WorkspaceMode;

/// Which surface a turn belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// An ordinary conversation. No files, no commands.
    Chat,
    /// A coding workspace.
    Code,
}

/// Ceiling on tool rounds, whatever the effort.
///
/// A model that has not finished after this many rounds is looping, and the
/// classic case — a search returning nothing and being retried verbatim — does
/// not get better with more attempts.
pub const MAX_ROUNDS_CEILING: usize = 16;

/// What Ultra is allowed, which is the real ceiling.
///
/// Two and a half times Max. A long agentic run — read the project, change six
/// files, build, fix, build again, run the tests, fix, run again — genuinely
/// costs this many rounds, and the previous ceiling turned that into a turn that
/// stopped halfway and reported partial work as finished.
pub const ULTRA_ROUNDS: usize = 40;

/// What one turn is allowed to do.
#[derive(Debug, Clone)]
pub struct Plan {
    /// How many times the model may call tools and be asked again.
    pub max_tool_rounds: usize,
    /// Skills added on top of the user's own selection, already de-duplicated
    /// against it.
    pub added_skills: Vec<&'static Skill>,
    /// One line for the interface, so the effort control can say what it did
    /// rather than being a mystery dial.
    pub summary: String,
}

/// What the turn is being planned for.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub effort: Effort,
    pub surface: Surface,
    /// The workspace this turn runs in, when there is one. Its root is what the
    /// language detection reads.
    pub workspace: Option<(&'a Path, WorkspaceMode)>,
    /// Whether the user has auto-orchestration on for this surface.
    pub auto: bool,
}

/// Resolve an effort level into a plan.
pub fn plan(request: &Request<'_>, already_enabled: &[&'static Skill]) -> Plan {
    let rounds = rounds_for(request.effort, request.surface);

    if !request.auto {
        return Plan {
            max_tool_rounds: rounds,
            added_skills: Vec::new(),
            summary: format!("{} effort · up to {rounds} tool rounds", request.effort.as_str()),
        };
    }

    let mut added: Vec<&'static Skill> = Vec::new();

    // The essentials, whenever there is a project to work in. These are not a
    // function of effort: reading a file before editing it is not something to
    // do more of at high effort and less of at low.
    if let Some((root, mode)) = request.workspace {
        for skill in skills::essentials() {
            // Telling a model to run the tests when it has no way to run them is
            // an instruction it cannot follow, and a small model asked to do the
            // impossible tends to claim it did.
            if skill.slug == "running-code" && !mode.allows(crate::workspace::ToolRisk::Execute) {
                continue;
            }
            push_new(&mut added, already_enabled, skill);
        }

        // Language skills, from what is actually in the folder.
        if request.effort >= Effort::Balanced {
            for slug in detect_languages(root) {
                if let Some(skill) = skills::find(slug) {
                    push_new(&mut added, already_enabled, skill);
                }
            }
        }

        // Craft skills, as the budget allows.
        if request.effort >= Effort::High {
            for slug in ["reading-errors", "planning-the-work"] {
                if let Some(skill) = skills::find(slug) {
                    push_new(&mut added, already_enabled, skill);
                }
            }
        }
        if request.effort >= Effort::Max {
            for slug in ["using-the-terminal", "checking-it-visually", "dependencies"] {
                if let Some(skill) = skills::find(slug) {
                    push_new(&mut added, already_enabled, skill);
                }
            }
        }

        // Ultra stops rationing. Every coding skill goes in, plus the practice
        // and design guidance that shapes the decisions rather than the syntax —
        // which is the difference between code that compiles and code somebody
        // would merge.
        if request.effort == Effort::Ultra {
            for skill in skills::SKILLS {
                let wanted = matches!(
                    skill.category,
                    skills::SkillCategory::Coding | skills::SkillCategory::Design
                ) || matches!(
                    skill.slug,
                    "code-review" | "debugging" | "testing" | "security" | "architecture"
                        | "refactoring" | "performance" | "error-handling" | "git"
                );
                if wanted {
                    push_new(&mut added, already_enabled, skill);
                }
            }
        }
    } else if request.effort >= Effort::Max {
        // Chat gets one thing, and only at the top of the dial: the instruction
        // to work through a problem in steps, which is the one piece of guidance
        // that measurably helps a small model on anything with arithmetic or
        // several constraints in it.
        if let Some(skill) = skills::find("step-by-step") {
            push_new(&mut added, already_enabled, skill);
        }
    }

    Plan {
        summary: describe(request.effort, rounds, &added),
        max_tool_rounds: rounds,
        added_skills: added,
    }
}

fn push_new(
    into: &mut Vec<&'static Skill>,
    already_enabled: &[&'static Skill],
    skill: &'static Skill,
) {
    let held = already_enabled.iter().any(|have| have.slug == skill.slug)
        || into.iter().any(|have| have.slug == skill.slug);
    if !held {
        into.push(skill);
    }
}

fn describe(effort: Effort, rounds: usize, added: &[&'static Skill]) -> String {
    if added.is_empty() {
        return format!("{} effort · up to {rounds} tool rounds", effort.as_str());
    }

    let names: Vec<&str> = added.iter().map(|skill| skill.name).collect();
    format!(
        "{} effort · up to {rounds} tool rounds · added {}",
        effort.as_str(),
        names.join(", ")
    )
}

/// How many tool rounds an effort level buys.
///
/// A coding turn gets more than a chat at the same setting, because the unit of
/// work is different: reading three files and running a build is four rounds
/// before any answer exists, and a chat that has searched twice has usually
/// finished.
fn rounds_for(effort: Effort, surface: Surface) -> usize {
    let base = match (surface, effort) {
        (Surface::Chat, Effort::Low) => 2,
        (Surface::Chat, Effort::Balanced) => 4,
        (Surface::Chat, Effort::High) => 6,
        (Surface::Chat, Effort::Max) => 8,
        (Surface::Code, Effort::Low) => 4,
        (Surface::Code, Effort::Balanced) => 8,
        (Surface::Code, Effort::High) => 12,
        (Surface::Code, Effort::Max) => 16,
        (Surface::Code, Effort::Ultra) => ULTRA_ROUNDS,
        // Ultra is a coding level. Asked for in a chat — by a stored preference
        // carried over, say — it behaves as Max rather than as an error.
        (Surface::Chat, Effort::Ultra) => 8,
    };
    base.min(ULTRA_ROUNDS)
}

/// Marker files that say what a project is written in.
///
/// Only the top level is read. A `Cargo.toml` at the root is what the project
/// is; a `package.json` four directories down inside `examples/` is not, and
/// walking the whole tree to find one would both cost more and be wrong more.
const MARKERS: &[(&str, &[&str])] = &[
    ("Cargo.toml", &["rust"]),
    ("go.mod", &["go"]),
    ("package.json", &["typescript"]),
    ("tsconfig.json", &["typescript"]),
    ("deno.json", &["typescript"]),
    ("pyproject.toml", &["python"]),
    ("requirements.txt", &["python"]),
    ("setup.py", &["python"]),
    ("Pipfile", &["python"]),
    ("Gemfile", &["ruby"]),
    ("composer.json", &["php"]),
    ("pom.xml", &["java"]),
    ("build.gradle", &["java"]),
    ("build.gradle.kts", &["kotlin"]),
    ("Package.swift", &["swift"]),
    ("CMakeLists.txt", &["cpp"]),
    ("Makefile", &["shell"]),
];

/// Extra evidence found *inside* a marker file.
///
/// `package.json` says the project is JavaScript, which was already obvious. The
/// interesting question is which framework, and the dependency list answers it
/// exactly rather than by guessing from folder names.
const PACKAGE_HINTS: &[(&str, &str)] = &[
    ("\"react\"", "react"),
    ("\"react-dom\"", "react"),
    ("\"next\"", "react"),
    ("\"tailwindcss\"", "html-css"),
];

/// Which language skills this project's own files justify.
pub fn detect_languages(root: &Path) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();

    for (marker, slugs) in MARKERS {
        if !root.join(marker).exists() {
            continue;
        }
        for slug in *slugs {
            if !found.contains(slug) {
                found.push(slug);
            }
        }

        if *marker == "package.json" {
            if let Ok(manifest) = std::fs::read_to_string(root.join(marker)) {
                for (needle, slug) in PACKAGE_HINTS {
                    if manifest.contains(needle) && !found.contains(slug) {
                        found.push(slug);
                    }
                }
            }
        }
    }

    // Three is already 350 tokens of language guidance. A polyglot repository
    // that matched six would crowd out the conversation it was meant to help.
    found.truncate(3);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("kuro-orch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");
        for (name, contents) in files {
            std::fs::write(root.join(name), contents).expect("write");
        }
        root
    }

    fn request(effort: Effort, root: &Path, mode: WorkspaceMode) -> Request<'_> {
        Request {
            effort,
            surface: Surface::Code,
            workspace: Some((root, mode)),
            auto: true,
        }
    }

    #[test]
    fn effort_buys_tool_rounds_and_coding_gets_more_than_chat() {
        for surface in [Surface::Chat, Surface::Code] {
            let budgets: Vec<usize> = [Effort::Low, Effort::Balanced, Effort::High, Effort::Max]
                .iter()
                .map(|effort| rounds_for(*effort, surface))
                .collect();
            assert!(
                budgets.windows(2).all(|pair| pair[0] < pair[1]),
                "more effort must buy more rounds on {surface:?}"
            );
        }

        assert!(
            rounds_for(Effort::Balanced, Surface::Code)
                > rounds_for(Effort::Balanced, Surface::Chat),
            "reading files and running a build costs more rounds than searching twice"
        );
        assert!(rounds_for(Effort::Max, Surface::Code) <= MAX_ROUNDS_CEILING);
    }

    #[test]
    fn a_rust_project_gets_the_rust_skill_and_a_node_one_does_not() {
        let rust = project(&[("Cargo.toml", "[package]\nname = \"x\"\n")]);
        assert_eq!(detect_languages(&rust), vec!["rust"]);

        let node = project(&[("package.json", "{\"dependencies\":{}}")]);
        assert_eq!(detect_languages(&node), vec!["typescript"]);

        std::fs::remove_dir_all(&rust).ok();
        std::fs::remove_dir_all(&node).ok();
    }

    #[test]
    fn a_react_project_is_recognised_from_its_dependencies_rather_than_guessed() {
        let root = project(&[(
            "package.json",
            "{\"dependencies\":{\"react\":\"^18\",\"react-dom\":\"^18\"}}",
        )]);

        let found = detect_languages(&root);
        assert!(found.contains(&"typescript"));
        assert!(found.contains(&"react"), "got {found:?}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_folder_with_nothing_recognisable_gets_no_language_skills() {
        let root = project(&[("notes.txt", "hello")]);
        assert!(detect_languages(&root).is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_polyglot_repository_does_not_get_every_language_at_once() {
        let root = project(&[
            ("Cargo.toml", ""),
            ("go.mod", ""),
            ("pyproject.toml", ""),
            ("Gemfile", ""),
            ("pom.xml", ""),
        ]);

        assert!(
            detect_languages(&root).len() <= 3,
            "six language skills would crowd out the conversation"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_essentials_come_in_at_every_effort_level() {
        let root = project(&[("Cargo.toml", "")]);

        for effort in [Effort::Low, Effort::Balanced, Effort::High, Effort::Max] {
            let plan = plan(&request(effort, &root, WorkspaceMode::Agent), &[]);
            let slugs: Vec<&str> = plan.added_skills.iter().map(|skill| skill.slug).collect();

            assert!(
                slugs.contains(&"careful-edits"),
                "reading before editing is not something to do less of at low effort"
            );
        }

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn more_effort_brings_more_expertise() {
        let root = project(&[("Cargo.toml", "")]);

        let counts: Vec<usize> = [Effort::Low, Effort::Balanced, Effort::High, Effort::Max]
            .iter()
            .map(|effort| plan(&request(*effort, &root, WorkspaceMode::Agent), &[]).added_skills.len())
            .collect();

        assert!(
            counts.windows(2).all(|pair| pair[0] <= pair[1]),
            "effort should never remove expertise; got {counts:?}"
        );
        assert!(counts[3] > counts[0], "max should bring more than low");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_read_only_workspace_is_not_told_to_run_the_tests() {
        // It cannot, and a small model told to do the impossible claims it did.
        let root = project(&[("Cargo.toml", "")]);

        let planning = plan(&request(Effort::Max, &root, WorkspaceMode::Plan), &[]);
        let slugs: Vec<&str> = planning.added_skills.iter().map(|skill| skill.slug).collect();
        assert!(!slugs.contains(&"running-code"), "got {slugs:?}");

        let agent = plan(&request(Effort::Max, &root, WorkspaceMode::Agent), &[]);
        let agent_slugs: Vec<&str> = agent.added_skills.iter().map(|skill| skill.slug).collect();
        assert!(agent_slugs.contains(&"running-code"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_skill_the_user_already_chose_is_not_added_twice() {
        let root = project(&[("Cargo.toml", "")]);
        let rust = skills::find("rust").expect("rust");

        let plan = plan(&request(Effort::High, &root, WorkspaceMode::Agent), &[rust]);

        assert_eq!(
            plan.added_skills.iter().filter(|skill| skill.slug == "rust").count(),
            0,
            "the user's own selection is already in the brief"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn turning_orchestration_off_adds_nothing_but_still_budgets_rounds() {
        let root = project(&[("Cargo.toml", "")]);

        let plan = plan(
            &Request {
                auto: false,
                ..request(Effort::Max, &root, WorkspaceMode::Agent)
            },
            &[],
        );

        assert!(plan.added_skills.is_empty(), "somebody who curated a list gets their list");
        assert_eq!(plan.max_tool_rounds, rounds_for(Effort::Max, Surface::Code));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_chat_gets_no_coding_skills_however_hard_it_tries() {
        let plan = plan(
            &Request {
                effort: Effort::Max,
                surface: Surface::Chat,
                workspace: None,
                auto: true,
            },
            &[],
        );

        for skill in &plan.added_skills {
            assert_ne!(
                skill.category,
                skills::SkillCategory::Coding,
                "`{}` is about acting on a project a chat cannot reach",
                skill.slug
            );
        }
    }

    #[test]
    fn ultra_brings_everything_a_project_could_want() {
        let root = project(&[("Cargo.toml", "")]);

        let max = plan(&request(Effort::Max, &root, WorkspaceMode::Agent), &[]);
        let ultra = plan(&request(Effort::Ultra, &root, WorkspaceMode::Agent), &[]);

        assert!(
            ultra.added_skills.len() > max.added_skills.len(),
            "ultra is the level that stops rationing; got {} vs {}",
            ultra.added_skills.len(),
            max.added_skills.len()
        );

        let slugs: Vec<&str> = ultra.added_skills.iter().map(|skill| skill.slug).collect();
        for expected in ["code-review", "testing", "architecture", "ui-design", "accessibility"] {
            assert!(slugs.contains(&expected), "ultra should include `{expected}`: {slugs:?}");
        }

        assert!(
            ultra.max_tool_rounds > max.max_tool_rounds,
            "a long agentic run genuinely costs more rounds than Max allows"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn ultra_in_a_chat_degrades_rather_than_misbehaving() {
        // Reachable through a stored preference carried over from the Code page.
        // It should behave as the top chat level, not as an error and not by
        // handing a conversation forty rounds of tool use.
        let planned = plan(
            &Request {
                effort: Effort::Ultra,
                surface: Surface::Chat,
                workspace: None,
                auto: true,
            },
            &[],
        );

        assert_eq!(planned.max_tool_rounds, rounds_for(Effort::Max, Surface::Chat));
        for skill in &planned.added_skills {
            assert_ne!(skill.category, skills::SkillCategory::Coding);
        }
    }

    #[test]
    fn the_plan_says_what_it_did_so_the_dial_is_not_a_mystery() {
        let root = project(&[("Cargo.toml", "")]);

        let plan = plan(&request(Effort::High, &root, WorkspaceMode::Agent), &[]);

        assert!(plan.summary.contains("high"));
        assert!(plan.summary.contains("tool rounds"));
        assert!(plan.summary.contains("Rust"), "got: {}", plan.summary);

        std::fs::remove_dir_all(&root).ok();
    }
}
