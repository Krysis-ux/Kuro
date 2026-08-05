//! Finding things in a project.
//!
//! Two searches, because a model looking at an unfamiliar repository asks two
//! different questions: "what is in here" and "where is this string".
//!
//! Both skip the directories that make a naive walk useless — a `node_modules`
//! with forty thousand files under it will fill a context window with
//! dependency source before reaching any of the user's own code, and a `.git`
//! directory will fill it with compressed objects that are not text at all.

use std::path::{Path, PathBuf};

use crate::{KuroError, Result};

/// Directories never walked into.
///
/// Named rather than pattern-matched: the list is short, every entry is here for
/// a specific reason, and a rule broad enough to catch them all would also catch
/// directories a user legitimately wants read.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".svelte-kit",
    "__pycache__",
    ".venv",
    "venv",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".gradle",
    ".idea",
    ".vscode",
    "Pods",
    ".DS_Store",
];

/// Files larger than this are not searched. A minified bundle or a checked-in
/// binary matches nothing useful and costs the whole read.
const MAX_SEARCHABLE_BYTES: u64 = 2 * 1024 * 1024;

/// Ceiling on entries returned by a tree walk, so a huge repository still
/// answers rather than producing something no model can read.
const MAX_TREE_ENTRIES: usize = 600;

/// Ceiling on matches returned by a search.
const MAX_MATCHES: usize = 80;

/// How much of a matching line is kept.
const MAX_LINE_CHARS: usize = 240;

pub struct Match {
    pub path: PathBuf,
    pub line_number: usize,
    pub line: String,
}

/// Walk a project and list its files, relative to `root`.
///
/// Directories are included so an empty one is still visible, and the result is
/// sorted so repeated calls read the same way.
pub fn tree(root: &Path) -> Result<Vec<String>> {
    let mut found: Vec<String> = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    let mut truncated = false;

    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            if found.len() >= MAX_TREE_ENTRIES {
                truncated = true;
                break;
            }

            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if is_skipped(&name) {
                continue;
            }

            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let shown = relative.to_string_lossy().to_string();

            if file_type.is_dir() {
                found.push(format!("{shown}/"));
                pending.push(path);
            } else if file_type.is_file() {
                found.push(shown);
            }
        }
    }

    found.sort();
    if truncated {
        found.push(format!(
            "… listing stopped at {MAX_TREE_ENTRIES} entries. Search for a name instead."
        ));
    }
    Ok(found)
}

/// Find files whose path matches a shell-style pattern.
///
/// `*.rs`, `src/**/*.ts`, `*test*`. Deliberately not a regular expression: a
/// model writing an untested regex is the reliable way to get zero results and
/// conclude the code does not exist, and the patterns people actually reach for
/// when looking for files are glob-shaped anyway.
///
/// Matched against the path relative to the root, so `src/*.rs` means what it
/// looks like. A pattern with no slash in it is matched against the file's name
/// alone, because `*.rs` obviously means "any Rust file anywhere" and making
/// somebody write `**/*.rs` for that is a trap.
pub fn find_files(root: &Path, pattern: &str) -> Result<Vec<String>> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(KuroError::bad_request("a pattern is required"));
    }
    let name_only = !pattern.contains('/');

    let mut found: Vec<String> = tree(root)?
        .into_iter()
        .filter(|entry| !entry.ends_with('/') && !entry.starts_with('…'))
        .filter(|entry| {
            let candidate = if name_only {
                entry.rsplit('/').next().unwrap_or(entry)
            } else {
                entry.as_str()
            };
            glob_match(pattern, candidate)
        })
        .collect();

    found.sort();
    found.truncate(MAX_MATCHES);
    Ok(found)
}

/// Whether a glob pattern matches, supporting `*`, `**` and `?`.
///
/// Hand-rolled rather than pulled in as a dependency: the crate that would
/// replace it brings a `Pattern` type, an error enum and a compilation step for
/// a job done once per call on a few hundred short strings.
///
/// Matched a path segment at a time rather than character by character. The
/// first attempt here did the latter and got `**` wrong in a way worth
/// recording: a single flag tracked whether the *most recent* star could cross
/// a `/`, so in `src/**/*.rs` the inner `*` overwrote the outer `**`'s
/// backtracking point, and `src/db/models.rs` — the exact case `**` exists for —
/// failed to match. Splitting on `/` first makes that impossible to express: a
/// `**` segment consumes whole segments and a `*` never sees a separator at all.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let text: Vec<&str> = text.split('/').collect();
    match_segments(&pattern, &text)
}

/// Match a list of pattern segments against a list of path segments.
///
/// Recursive, and safely so: the depth is the number of `/` in a path, which is
/// a handful even in a deeply nested project.
fn match_segments(pattern: &[&str], text: &[&str]) -> bool {
    let Some((head, rest)) = pattern.split_first() else {
        return text.is_empty();
    };

    if *head == "**" {
        // Zero or more segments, zero included — otherwise `src/**/*.rs` is a
        // pattern that mysteriously skips the files directly inside `src`.
        return (0..=text.len()).any(|skip| match_segments(rest, &text[skip..]));
    }

    let Some((first, others)) = text.split_first() else {
        return false;
    };
    match_segment(head, first) && match_segments(rest, others)
}

/// Match one segment, where `*` means "any run of characters within this
/// segment" and `?` means exactly one.
fn match_segment(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();

    // Iterative with a single backtrack point, which is enough now that there
    // are no separators to reason about.
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut star_t) = (None, 0usize);

    while t < text.len() {
        if p < pattern.len()
            && pattern[p] != '*'
            && (pattern[p] == '?' || pattern[p] == text[t])
        {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            p += 1;
            star = Some(p);
            star_t = t;
        } else if let Some(resume) = star {
            p = resume;
            star_t += 1;
            t = star_t;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

/// One path per line, or a plain statement that nothing matched.
pub fn format_paths(pattern: &str, found: &[String]) -> String {
    if found.is_empty() {
        return format!(
            "No file matches `{pattern}`. Try a broader pattern, or project_tree to see \
             the layout."
        );
    }

    let mut out = format!(
        "{} file{} matching `{pattern}`:\n",
        found.len(),
        if found.len() == 1 { "" } else { "s" }
    );
    for path in found {
        out.push_str(&format!("\n{path}"));
    }
    out
}

/// Find a literal string in the project's text files.
///
/// Literal rather than a regular expression on purpose. A model writing a regex
/// it has not tested is a common way to get zero results and conclude the code
/// is absent, and "find every place this function is called" is a substring
/// question in practice.
pub fn find_text(root: &Path, needle: &str, case_sensitive: bool) -> Result<Vec<Match>> {
    let mut matches: Vec<Match> = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    let comparable = if case_sensitive {
        needle.to_string()
    } else {
        needle.to_lowercase()
    };

    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            if matches.len() >= MAX_MATCHES {
                return Ok(matches);
            }

            let name = entry.file_name();
            if is_skipped(&name.to_string_lossy()) {
                continue;
            }

            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_SEARCHABLE_BYTES {
                continue;
            }

            // A file that is not valid UTF-8 is binary for this purpose. Reading
            // it lossily would produce matches inside replacement characters.
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };

            for (index, line) in contents.lines().enumerate() {
                if matches.len() >= MAX_MATCHES {
                    return Ok(matches);
                }

                let haystack = if case_sensitive {
                    line.to_string()
                } else {
                    line.to_lowercase()
                };
                if !haystack.contains(&comparable) {
                    continue;
                }

                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                matches.push(Match {
                    path: relative,
                    line_number: index + 1,
                    line: truncate(line.trim_end()),
                });
            }
        }
    }

    Ok(matches)
}

/// Render matches the way a model can act on: path, line number, the line.
pub fn format_matches(needle: &str, matches: &[Match]) -> String {
    if matches.is_empty() {
        return format!("No file in this project contains `{needle}`.");
    }

    let mut out = format!(
        "{} {} for `{needle}`:\n\n",
        matches.len(),
        if matches.len() == 1 { "match" } else { "matches" }
    );
    for found in matches {
        out.push_str(&format!(
            "{}:{}: {}\n",
            found.path.display(),
            found.line_number,
            found.line
        ));
    }
    if matches.len() >= MAX_MATCHES {
        out.push_str("\n… more matches exist. Narrow the search.\n");
    }
    out
}

pub fn format_tree(root: &Path, entries: &[String]) -> String {
    if entries.is_empty() {
        return format!("`{}` is empty.", root.display());
    }
    format!(
        "{} entries under `{}`:\n\n{}",
        entries.len(),
        root.display(),
        entries.join("\n")
    )
}

fn is_skipped(name: &str) -> bool {
    SKIPPED_DIRS.contains(&name)
}

fn truncate(line: &str) -> String {
    if line.chars().count() <= MAX_LINE_CHARS {
        return line.to_string();
    }
    let kept: String = line.chars().take(MAX_LINE_CHARS).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_matches_within_a_segment_and_stops_at_a_slash() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("src/*.rs", "src/main.rs"));
        // The distinction that makes `**` worth having: a single star must not
        // reach across a directory boundary, or `src/*.rs` silently matches
        // everything under `src` however deep.
        assert!(!glob_match("src/*.rs", "src/db/models.rs"));
    }

    #[test]
    fn a_double_star_crosses_directories_including_none_at_all() {
        assert!(glob_match("src/**/*.rs", "src/db/models.rs"));
        assert!(glob_match("src/**/*.rs", "src/db/deep/nested/thing.rs"));
        // Zero directories too, or `src/**/*.rs` is a pattern that mysteriously
        // skips the files directly inside `src`.
        assert!(glob_match("src/**/*.rs", "src/main.rs"));
    }

    #[test]
    fn a_question_mark_matches_exactly_one_character() {
        assert!(glob_match("v?.rs", "v1.rs"));
        assert!(!glob_match("v?.rs", "v10.rs"));
        assert!(!glob_match("v?.rs", "v.rs"));
    }

    #[test]
    fn a_pattern_matching_nothing_says_so_rather_than_matching_everything() {
        assert!(!glob_match("*.rs", "main.ts"));
        assert!(!glob_match("test_*", "main.rs"));
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn a_pattern_without_a_slash_matches_the_name_at_any_depth() {
        // `*.rs` obviously means "any Rust file"; making somebody write
        // `**/*.rs` for that is a trap, and the one they will fall into first.
        let root = sample_project();

        let found = find_files(&root, "*.rs").expect("find");

        assert!(found.contains(&"src/main.rs".to_string()), "got {found:?}");
        assert!(found.contains(&"src/greet.rs".to_string()));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn finding_files_skips_the_directories_a_walk_should_never_enter() {
        // The same reason `tree` skips them: `node_modules` would fill the
        // answer with dependency source before reaching the user's own code.
        let root = sample_project();

        let found = find_files(&root, "*").expect("find");

        assert!(!found.iter().any(|path| path.contains("node_modules")), "got {found:?}");
        assert!(!found.iter().any(|path| path.contains(".git")));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_pattern_that_matches_nothing_suggests_what_to_do_instead() {
        let root = sample_project();

        let found = find_files(&root, "*.cobol").expect("find");
        let rendered = format_paths("*.cobol", &found);

        assert!(found.is_empty());
        assert!(rendered.contains("No file matches"));
        assert!(rendered.contains("project_tree"), "a dead end needs a way out");

        std::fs::remove_dir_all(&root).ok();
    }

    /// A small project with the kinds of directory that ruin a naive walk.
    fn sample_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!("kuro-search-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::create_dir_all(root.join("node_modules/left-pad")).expect("mkdir");
        std::fs::create_dir_all(root.join(".git/objects")).expect("mkdir");

        std::fs::write(root.join("src/main.rs"), "fn main() {\n    greet();\n}\n").expect("write");
        std::fs::write(root.join("src/greet.rs"), "pub fn greet() {\n    // hello\n}\n")
            .expect("write");
        std::fs::write(root.join("README.md"), "# Sample\n").expect("write");
        std::fs::write(root.join("node_modules/left-pad/index.js"), "greet()").expect("write");
        std::fs::write(root.join(".git/objects/abc"), "greet()").expect("write");
        root
    }

    #[test]
    fn the_tree_lists_project_files_and_skips_dependency_directories() {
        let root = sample_project();

        let entries = tree(&root).expect("tree");

        assert!(entries.iter().any(|e| e == "src/main.rs"));
        assert!(entries.iter().any(|e| e == "README.md"));
        assert!(entries.iter().any(|e| e == "src/"));
        assert!(
            !entries.iter().any(|e| e.contains("node_modules")),
            "a dependency tree would crowd out the user's own code"
        );
        assert!(!entries.iter().any(|e| e.contains(".git")));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn search_reports_the_file_and_line_of_every_hit() {
        let root = sample_project();

        let found = find_text(&root, "greet", false).expect("search");

        assert!(found.len() >= 2, "got {}", found.len());
        assert!(found.iter().all(|m| !m.path.starts_with("node_modules")));
        let main = found
            .iter()
            .find(|m| m.path.ends_with("main.rs"))
            .expect("main.rs should match");
        assert_eq!(main.line_number, 2);
        assert!(main.line.contains("greet()"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn search_can_be_case_sensitive() {
        let root = sample_project();

        assert!(!find_text(&root, "GREET", false).expect("search").is_empty());
        assert!(find_text(&root, "GREET", true).expect("search").is_empty());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_search_with_no_hits_says_so_rather_than_returning_nothing() {
        let root = sample_project();

        let found = find_text(&root, "kubernetes", false).expect("search");
        let rendered = format_matches("kubernetes", &found);

        assert!(rendered.contains("No file"), "got: {rendered}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn long_lines_are_truncated_without_splitting_a_character() {
        let root = std::env::temp_dir().join(format!("kuro-long-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(root.join("wide.txt"), format!("{}needle", "日".repeat(500)))
            .expect("write");

        let found = find_text(&root, "needle", false).expect("search");

        assert_eq!(found.len(), 1);
        assert!(found[0].line.ends_with('…'));
        assert!(found[0].line.chars().count() <= MAX_LINE_CHARS + 1);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn binary_files_are_skipped_rather_than_read_lossily() {
        let root = std::env::temp_dir().join(format!("kuro-binary-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(root.join("blob.bin"), [0xff, 0xfe, 0x00, 0x01]).expect("write");
        std::fs::write(root.join("ok.txt"), "needle").expect("write");

        let found = find_text(&root, "needle", false).expect("search");

        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("ok.txt"));

        std::fs::remove_dir_all(&root).ok();
    }
}
