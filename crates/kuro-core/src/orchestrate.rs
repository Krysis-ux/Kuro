
use std::path::Path;

use serde::Serialize;

use crate::settings::Effort;
use crate::skills::{self, Skill};
use crate::workspace::WorkspaceMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Chat,
    Code,
}

pub const MAX_ROUNDS_CEILING: usize = 16;

pub const ULTRA_ROUNDS: usize = 40;

#[derive(Debug, Clone)]
pub struct Plan {
    pub max_tool_rounds: usize,
    pub skills: Vec<&'static Skill>,
    pub added_skills: Vec<&'static Skill>,
    pub trimmed_skills: Vec<&'static Skill>,
    pub budget_tokens: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub effort: Effort,
    pub surface: Surface,
    pub workspace: Option<(&'a Path, WorkspaceMode)>,
    pub auto: bool,
    pub message: &'a str,
    pub pinned: &'a [String],
}

pub fn budget_for(effort: Effort, surface: Surface) -> usize {
    match (surface, effort) {
        (Surface::Chat, Effort::Low) => 200,
        (Surface::Chat, Effort::Balanced) => 500,
        (Surface::Chat, Effort::High) => 900,
        (Surface::Chat, Effort::Max | Effort::Ultra) => 1_400,
        (Surface::Code, Effort::Low) => 400,
        (Surface::Code, Effort::Balanced) => 1_000,
        (Surface::Code, Effort::High) => 1_800,
        (Surface::Code, Effort::Max) => 3_000,
        (Surface::Code, Effort::Ultra) => 8_000,
    }
}

const TRIGGERS: &[(&str, &[&str])] = &[
    ("rust", &["rust", "cargo", "clippy", "rustc", "crate"]),
    ("python", &["python", "py", "pip", "django", "flask", "pandas", "numpy", "pytest"]),
    ("typescript", &["typescript", "ts", "tsx", "javascript", "js", "node", "npm", "deno"]),
    ("go", &["go", "golang", "goroutine", "gofmt"]),
    ("sql", &["sql", "postgres", "postgresql", "sqlite", "mysql", "query", "schema", "index"]),
    ("shell", &["shell", "bash", "zsh", "sh", "script"]),
    ("java", &["java", "maven", "gradle", "spring"]),
    ("csharp", &["csharp", "dotnet", "nuget", "asp"]),
    ("cpp", &["cpp", "cmake", "clang", "gcc"]),
    ("swift", &["swift", "swiftui", "xcode", "ios"]),
    ("kotlin", &["kotlin", "android", "compose"]),
    ("php", &["php", "laravel", "composer", "symfony"]),
    ("ruby", &["ruby", "rails", "gem", "bundler"]),
    ("html-css", &["html", "css", "stylesheet", "tailwind", "flexbox", "grid", "layout"]),
    ("react", &["react", "jsx", "hook", "hooks", "component", "usestate", "useeffect"]),
    ("testing", &["test", "tests", "testing", "coverage", "assert", "spec", "mock"]),
    ("debugging", &["debug", "debugging", "bug", "crash", "broken", "failing", "why"]),
    ("security", &["security", "secure", "auth", "vulnerability", "injection", "xss", "csrf"]),
    ("performance", &["performance", "slow", "slower", "fast", "faster", "latency", "optimise",
                      "optimize", "profiling", "memory"]),
    ("code-review", &["review", "reviewing", "feedback", "critique"]),
    ("refactoring", &["refactor", "refactoring", "cleanup", "tidy", "simplify"]),
    ("architecture", &["architecture", "design", "structure", "module", "boundary"]),
    ("api-design", &["api", "endpoint", "rest", "route", "http"]),
    ("git", &["git", "commit", "branch", "merge", "rebase", "conflict"]),
    ("error-handling", &["error", "errors", "exception", "panic", "failure"]),
    ("accessibility", &["accessibility", "a11y", "aria", "screenreader", "contrast"]),
    ("ui-design", &["ui", "ux", "interface", "spacing", "typography", "colour", "color"]),
    ("data-modelling", &["model", "modelling", "modeling", "entity", "relation", "migration"]),
    ("recursive-learning", &["again", "retry", "still", "keeps", "remember", "learned", "loop"]),
    ("root-cause", &["why", "cause", "root", "broken", "failing", "regression"]),
    ("staying-in-scope", &["just", "only", "minimal", "small", "quick", "scope"]),
    ("matching-the-codebase", &["convention", "style", "consistent", "existing", "idiom"]),
    ("honest-reporting", &["verify", "verified", "actually", "confirm", "sure", "check"]),
    ("context-economy", &["large", "huge", "big", "context"]),
    ("tool-batching", &["parallel", "batch", "faster", "rounds"]),
    ("asking-well", &["ambiguous", "unclear", "assume", "unsure"]),
];

pub fn plan(request: &Request<'_>, already_enabled: &[&'static Skill]) -> Plan {
    let rounds = rounds_for(request.effort, request.surface);
    let budget = budget_for(request.effort, request.surface);

    if !request.auto {
        let mut skills = already_enabled.to_vec();
        for slug in request.pinned {
            if let Some(skill) = skills::find(slug) {
                if !skills.iter().any(|have| have.slug == skill.slug) {
                    skills.push(skill);
                }
            }
        }

        return Plan {
            max_tool_rounds: rounds,
            skills,
            added_skills: Vec::new(),
            trimmed_skills: Vec::new(),
            budget_tokens: budget,
            summary: format!("{} effort · up to {rounds} tool rounds", request.effort.as_str()),
        };
    }

    let mut added: Vec<&'static Skill> = Vec::new();
    let mut from_project: Vec<&'static str> = Vec::new();

    if let Some((root, mode)) = request.workspace {
        for skill in skills::essentials() {
            if skill.slug == "running-code" && !mode.allows(crate::workspace::ToolRisk::Execute) {
                continue;
            }
            push_new(&mut added, already_enabled, skill);
        }

        if request.effort >= Effort::Balanced {
            for slug in detect_languages(root) {
                if let Some(skill) = skills::find(slug) {
                    from_project.push(skill.slug);
                    push_new(&mut added, already_enabled, skill);
                }
            }
        }

        if request.effort >= Effort::Balanced {
            for slug in ["context-economy", "staying-in-scope", "honest-reporting"] {
                if let Some(skill) = skills::find(slug) {
                    push_new(&mut added, already_enabled, skill);
                }
            }
        }
        if request.effort >= Effort::High {
            for slug in ["reading-errors", "planning-the-work", "tool-batching", "root-cause"] {
                if let Some(skill) = skills::find(slug) {
                    push_new(&mut added, already_enabled, skill);
                }
            }
        }
        if request.effort >= Effort::Max {
            for slug in [
                "using-the-terminal",
                "checking-it-visually",
                "dependencies",
                "matching-the-codebase",
                "asking-well",
                "recursive-learning",
            ] {
                if let Some(skill) = skills::find(slug) {
                    push_new(&mut added, already_enabled, skill);
                }
            }
        }

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
        if let Some(skill) = skills::find("step-by-step") {
            push_new(&mut added, already_enabled, skill);
        }
    }

    for slug in request.pinned {
        if let Some(skill) = skills::find(slug) {
            push_new(&mut added, already_enabled, skill);
        }
    }

    let chosen = Selection {
        enabled: already_enabled,
        added: &added,
        from_project: &from_project,
    };
    let (kept, trimmed) = fit_to_budget(&chosen, request, budget);

    let added_skills: Vec<&'static Skill> = kept
        .iter()
        .copied()
        .filter(|skill| !already_enabled.iter().any(|have| have.slug == skill.slug))
        .collect();

    Plan {
        summary: describe(request.effort, rounds, &added_skills, &trimmed),
        max_tool_rounds: rounds,
        skills: kept,
        added_skills,
        trimmed_skills: trimmed,
        budget_tokens: budget,
    }
}

struct Selection<'a> {
    enabled: &'a [&'static Skill],
    added: &'a [&'static Skill],
    from_project: &'a [&'static str],
}

fn fit_to_budget(
    selection: &Selection<'_>,
    request: &Request<'_>,
    budget: usize,
) -> (Vec<&'static Skill>, Vec<&'static Skill>) {
    let words = tokenise(request.message);

    let mut ranked: Vec<(u8, usize, &'static Skill)> = Vec::new();
    let mut seen: Vec<&'static str> = Vec::new();

    let candidates = selection
        .enabled
        .iter()
        .map(|skill| (*skill, true))
        .chain(selection.added.iter().map(|skill| (*skill, false)));

    for (skill, chosen) in candidates {
        if seen.contains(&skill.slug) {
            continue;
        }
        seen.push(skill.slug);

        let tier = if skill.essential || request.pinned.iter().any(|slug| slug == skill.slug) {
            0
        } else if mentions(skill, &words) {
            1
        } else if selection.from_project.contains(&skill.slug) {
            2
        } else if chosen {
            3
        } else {
            4
        };
        ranked.push((tier, skill.approx_tokens, skill));
    }

    ranked.sort_by_key(|(tier, cost, _)| (*tier, *cost));

    let mut spent = 0usize;
    let mut kept = Vec::new();
    let mut trimmed = Vec::new();

    for (tier, cost, skill) in ranked {
        if tier == 0 {
            kept.push(skill);
            continue;
        }
        if spent + cost > budget {
            trimmed.push(skill);
            continue;
        }
        spent += cost;
        kept.push(skill);
    }

    (kept, trimmed)
}

fn mentions(skill: &Skill, words: &[String]) -> bool {
    let triggers = TRIGGERS
        .iter()
        .find(|(slug, _)| *slug == skill.slug)
        .map(|(_, words)| *words);

    match triggers {
        Some(triggers) => triggers.iter().any(|trigger| words.iter().any(|word| word == trigger)),
        None => skill
            .slug
            .split('-')
            .filter(|part| part.len() >= 3)
            .any(|part| words.iter().any(|word| word == part)),
    }
}

fn tokenise(message: &str) -> Vec<String> {
    message
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
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

fn describe(
    effort: Effort,
    rounds: usize,
    added: &[&'static Skill],
    trimmed: &[&'static Skill],
) -> String {
    let mut out = format!("{} effort · up to {rounds} tool rounds", effort.as_str());

    if !added.is_empty() {
        let names: Vec<&str> = added.iter().map(|skill| skill.name).collect();
        out.push_str(&format!(" · added {}", names.join(", ")));
    }

    if !trimmed.is_empty() {
        out.push_str(&format!(
            " · {} left out for room",
            trimmed.len()
        ));
    }

    out
}

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
        (Surface::Chat, Effort::Ultra) => 8,
    };
    base.min(ULTRA_ROUNDS)
}

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

const PACKAGE_HINTS: &[(&str, &str)] = &[
    ("\"react\"", "react"),
    ("\"react-dom\"", "react"),
    ("\"next\"", "react"),
    ("\"tailwindcss\"", "html-css"),
];

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
            message: "",
            pinned: &[],
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
                message: "",
                pinned: &[],
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
        let planned = plan(
            &Request {
                effort: Effort::Ultra,
                surface: Surface::Chat,
                workspace: None,
                auto: true,
                message: "",
                pinned: &[],
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

    fn everything() -> Vec<&'static Skill> {
        skills::selectable()
    }

    #[test]
    fn a_brief_never_costs_more_than_its_budget_even_with_every_skill_switched_on() {
        let root = project(&[("Cargo.toml", ""), ("package.json", "{}")]);

        for effort in [Effort::Low, Effort::Balanced, Effort::High, Effort::Max, Effort::Ultra] {
            let planned = plan(&request(effort, &root, WorkspaceMode::Agent), &everything());

            let spent: usize = planned
                .skills
                .iter()
                .filter(|skill| !skill.essential)
                .map(|skill| skill.approx_tokens)
                .sum();

            assert!(
                spent <= planned.budget_tokens,
                "{} spent {spent} against a budget of {}",
                effort.as_str(),
                planned.budget_tokens
            );
        }

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn switching_everything_on_is_survivable_rather_than_ruinous() {
        let root = project(&[("Cargo.toml", "")]);
        let all = everything();

        let planned = plan(&request(Effort::Balanced, &root, WorkspaceMode::Agent), &all);

        let everything_cost = skills::approx_tokens(&all);
        let sent_cost = skills::approx_tokens(&planned.skills);

        assert!(
            sent_cost < everything_cost / 2,
            "sent {sent_cost} of {everything_cost} — the selection is not selecting"
        );
        assert!(!planned.skills.is_empty(), "and it still has to send something");
    }

    #[test]
    fn the_essentials_are_never_traded_away_to_make_room() {
        let root = project(&[("Cargo.toml", "")]);

        let planned = plan(&request(Effort::Low, &root, WorkspaceMode::Agent), &everything());

        for essential in skills::essentials() {
            if essential.slug == "running-code" {
                continue;
            }
            assert!(
                planned.skills.iter().any(|kept| kept.slug == essential.slug),
                "`{}` was trimmed",
                essential.slug
            );
        }

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn turning_orchestration_off_sends_the_users_list_verbatim() {
        let root = project(&[("Cargo.toml", "")]);
        let chosen = everything();

        let mut asked = request(Effort::Low, &root, WorkspaceMode::Agent);
        asked.auto = false;

        let planned = plan(&asked, &chosen);

        assert_eq!(planned.skills.len(), chosen.len());
        assert!(planned.trimmed_skills.is_empty());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_crowded_brief_says_what_it_left_out_rather_than_dropping_it_silently() {
        let root = project(&[("Cargo.toml", "")]);

        let planned = plan(&request(Effort::Low, &root, WorkspaceMode::Agent), &everything());

        assert!(!planned.trimmed_skills.is_empty(), "nothing was trimmed to check");
        assert!(
            planned.summary.contains("left out"),
            "the difference between what is switched on and what was sent has to \
             be visible, or a full brief looks like a broken switch — got: {}",
            planned.summary
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_words_of_the_request_decide_what_survives_a_trim() {
        let root = project(&[("Cargo.toml", "")]);

        let mut asked = request(Effort::Low, &root, WorkspaceMode::Agent);
        asked.message = "my postgres query is slow, can you help me add an index";

        let planned = plan(&asked, &everything());
        let kept = |slug: &str| planned.skills.iter().any(|skill| skill.slug == slug);

        assert!(kept("sql"), "the question is about SQL: {}", planned.summary);
        assert!(kept("performance"), "and about speed: {}", planned.summary);
        assert!(!kept("ruby"));
        assert!(!kept("accessibility"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_skill_named_on_the_message_survives_any_budget() {
        let root = project(&[("package.json", "{}")]);

        let pinned = vec!["rust".to_string(), "security".to_string()];
        let mut asked = request(Effort::Low, &root, WorkspaceMode::Agent);
        asked.pinned = &pinned;
        asked.message = "make this page prettier";

        let planned = plan(&asked, &everything());

        for slug in &pinned {
            assert!(
                planned.skills.iter().any(|skill| skill.slug == *slug),
                "`{slug}` was named and still got trimmed: {}",
                planned.summary
            );
        }

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn naming_a_skill_works_without_switching_it_on_first() {
        let root = project(&[("package.json", "{}")]);

        let pinned = vec!["ruby".to_string()];
        let mut asked = request(Effort::Balanced, &root, WorkspaceMode::Agent);
        asked.pinned = &pinned;

        let planned = plan(&asked, &[]);

        assert!(planned.skills.iter().any(|skill| skill.slug == "ruby"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_project_outranks_a_switch_flipped_months_ago() {
        let root = project(&[("Cargo.toml", "")]);

        let mut asked = request(Effort::Balanced, &root, WorkspaceMode::Agent);
        asked.message = "tidy this up";

        let planned = plan(&asked, &everything());

        assert!(
            planned.skills.iter().any(|skill| skill.slug == "rust"),
            "got: {}",
            planned.summary
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_word_inside_another_word_does_not_summon_a_language() {
        let words = tokenise("I am going to categorise the algorithm");
        let go = skills::find("go").expect("go");

        assert!(!mentions(go, &words));
        assert!(mentions(go, &tokenise("write me a go server")));
    }

    #[test]
    fn matching_ignores_case_and_punctuation() {
        let rust = skills::find("rust").expect("rust");

        assert!(mentions(rust, &tokenise("Why won't CARGO build?")));
        assert!(mentions(rust, &tokenise("rust: lifetimes")));
    }

    #[test]
    fn a_skill_outside_the_trigger_table_still_answers_to_its_own_name() {
        let skill = skills::find("careful-edits").expect("careful-edits");
        assert!(mentions(skill, &tokenise("be careful with those edits")));
        assert!(!mentions(skill, &tokenise("write me a poem")));
    }
}
