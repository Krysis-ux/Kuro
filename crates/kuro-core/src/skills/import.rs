//! Pulling skills out of a GitHub repository.
//!
//! Reads the repository's tree through the API (one call), then fetches each
//! `SKILL.md`, instead of cloning: most of a skills repository is not skills, and
//! cloning needs `git` and leaves a checkout behind.
//!
//! Only `SKILL.md` is imported. Importing a skill's `scripts/` would put
//! someone else's executable code where a model can call it, from a URL with no
//! review step. A skill that references scripts is flagged via
//! [`Found::wants_scripts`] instead.

use serde::Deserialize;

use crate::db::UserSkillRecord;
use crate::{KuroError, Result};

use super::custom::{estimate_tokens, parse_skill_md, ParsedSkill};

/// Most `SKILL.md` files taken from one repository (more would flood the palette).
pub const MAX_PER_REPO: usize = 60;

/// A repository, as the tree API needs it named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
    /// `None` when the URL did not name one, in which case the default is used.
    pub branch: Option<String>,
}

impl Repo {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// Read a GitHub URL: browser URL, clone URL (`.git`), `tree/<branch>` link, or
/// bare `owner/name`.
pub fn parse_repo(raw: &str) -> Result<Repo> {
    let trimmed = raw.trim().trim_end_matches('/');

    let matched = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("git@github.com:"))
        .or_else(|| trimmed.strip_prefix("github.com/"));

    // A bare `owner/name` is allowed, but anything with a scheme or host must
    // have matched a form above (else `https://example.com` parses as owner
    // `https:`).
    let path = match matched {
        Some(rest) => rest,
        None if trimmed.contains("://") || trimmed.contains('@') => {
            return Err(KuroError::bad_request(format!(
                "`{raw}` is not a GitHub repository. It should look like \
                 https://github.com/owner/name."
            )))
        }
        None => trimmed,
    };

    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/').filter(|part| !part.is_empty());

    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();

    if owner.is_empty() || name.is_empty() {
        return Err(KuroError::bad_request(format!(
            "`{raw}` is not a GitHub repository. It should look like \
             https://github.com/owner/name."
        )));
    }

    // `.../tree/<branch>/...` imports that branch.
    let branch = match parts.next() {
        Some("tree") => parts.next().map(str::to_string),
        _ => None,
    };

    Ok(Repo {
        owner: owner.to_string(),
        name: name.to_string(),
        branch,
    })
}

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

/// One skill found in a repository, and what it cost to notice.
#[derive(Debug, Clone)]
pub struct Found {
    pub parsed: ParsedSkill,
    /// Where in the repository it came from, shown so the user can check it.
    pub path: String,
    /// True when the file expects `scripts/` that were deliberately not taken.
    pub wants_scripts: bool,
}

/// Find and read every `SKILL.md` in a repository.
pub async fn fetch_skills(client: &reqwest::Client, repo: &Repo) -> Result<Vec<Found>> {
    let branch = match &repo.branch {
        Some(named) => named.clone(),
        None => default_branch(client, repo).await?,
    };

    let url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        repo.owner, repo.name, branch
    );

    let response = client
        .get(&url)
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| KuroError::other(format!("could not reach GitHub: {error}")))?;

    if !response.status().is_success() {
        return Err(github_error(response.status(), &repo.slug()));
    }

    let tree: TreeResponse = response
        .json()
        .await
        .map_err(|error| KuroError::other(format!("GitHub sent something unreadable: {error}")))?;

    let mut paths: Vec<String> = tree
        .tree
        .into_iter()
        .filter(|entry| entry.kind == "blob")
        .map(|entry| entry.path)
        .filter(|path| is_skill_file(path))
        .collect();

    // Shallower paths first, so the cap keeps the obvious skills.
    paths.sort_by_key(|path| (path.matches('/').count(), path.clone()));

    if paths.is_empty() {
        return Err(KuroError::bad_request(format!(
            "{} has no SKILL.md in it{}. A skills repository keeps each skill in its own \
             SKILL.md; this one may keep them somewhere else, or not be a skills repository.",
            repo.slug(),
            if tree.truncated {
                " in the part GitHub returned (the repository is very large)"
            } else {
                ""
            }
        )));
    }

    let mut found: Vec<Found> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for path in paths.into_iter().take(MAX_PER_REPO) {
        let raw = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            repo.owner, repo.name, branch, path
        );

        let Ok(body) = client.get(&raw).send().await else {
            continue;
        };
        if !body.status().is_success() {
            continue;
        }
        let Ok(text) = body.text().await else { continue };

        // The containing directory is the skill's name in every common layout.
        let fallback = path
            .rsplit('/')
            .nth(1)
            .unwrap_or(&path)
            .to_string();

        match parse_skill_md(&text, &fallback) {
            Ok(parsed) => {
                // Repositories mirror one skill under `.claude/`, `.cursor/`, etc.
                // Dedupe by slug (not directory), since a lone copy under
                // `.claude/skills/` must still import.
                if seen.iter().any(|held| held == &parsed.slug) {
                    continue;
                }
                seen.push(parsed.slug.clone());
                found.push(Found {
                    wants_scripts: mentions_scripts(&parsed.instructions),
                    parsed,
                    path,
                });
            }
            // One malformed file should not fail an import of forty.
            Err(error) => tracing::debug!(%path, %error, "skipped an unreadable SKILL.md"),
        }
    }

    if found.is_empty() {
        return Err(KuroError::bad_request(format!(
            "{} has SKILL.md files but none of them could be read as a skill.",
            repo.slug()
        )));
    }

    Ok(found)
}

/// Ask GitHub for the default branch (guessing `main` 404s on `master` repos,
/// which reads as "no such repository").
async fn default_branch(client: &reqwest::Client, repo: &Repo) -> Result<String> {
    #[derive(Deserialize)]
    struct RepoResponse {
        default_branch: String,
    }

    let url = format!("https://api.github.com/repos/{}/{}", repo.owner, repo.name);
    let response = client
        .get(&url)
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| KuroError::other(format!("could not reach GitHub: {error}")))?;

    if !response.status().is_success() {
        return Err(github_error(response.status(), &repo.slug()));
    }

    let body: RepoResponse = response
        .json()
        .await
        .map_err(|error| KuroError::other(format!("GitHub sent something unreadable: {error}")))?;

    Ok(body.default_branch)
}

fn github_error(status: reqwest::StatusCode, slug: &str) -> KuroError {
    match status.as_u16() {
        404 => KuroError::not_found(format!(
            "{slug} — check the spelling, and note that a private repository cannot be read \
             without a token"
        )),
        403 | 429 => KuroError::other(
            "GitHub is rate-limiting this machine. Unauthenticated requests are capped at \
             sixty an hour; wait a few minutes and try again."
                .to_string(),
        ),
        other => KuroError::other(format!("GitHub answered {other} for {slug}")),
    }
}

/// Whether a path is a skill definition (`SKILL.md`, case-insensitive since some
/// repositories use `skill.md`).
fn is_skill_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
}

/// Whether the instructions expect files this import does not take.
fn mentions_scripts(instructions: &str) -> bool {
    let lowered = instructions.to_lowercase();
    ["scripts/", "./script", "python scripts", "run the script"]
        .iter()
        .any(|needle| lowered.contains(needle))
}

/// Turn a parsed skill into the row that will be stored.
pub fn to_record(found: &Found, source: &str) -> UserSkillRecord {
    UserSkillRecord {
        slug: found.parsed.slug.clone(),
        name: found.parsed.name.clone(),
        blurb: found.parsed.blurb.clone(),
        category: found.parsed.category.as_str().to_string(),
        approx_tokens: estimate_tokens(&found.parsed.instructions),
        instructions: found.parsed.instructions.clone(),
        source: source.to_string(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_of_github_url_people_paste_is_understood() {
        let expected = Repo {
            owner: "vercel-labs".to_string(),
            name: "skills".to_string(),
            branch: None,
        };

        for raw in [
            "https://github.com/vercel-labs/skills",
            "https://github.com/vercel-labs/skills.git",
            "https://github.com/vercel-labs/skills/",
            "github.com/vercel-labs/skills",
            "git@github.com:vercel-labs/skills.git",
            "vercel-labs/skills",
        ] {
            assert_eq!(parse_repo(raw).expect(raw), expected, "failed on {raw}");
        }
    }

    #[test]
    fn a_branch_link_imports_that_branch() {
        let repo = parse_repo("https://github.com/anthropics/skills/tree/next").expect("parsed");
        assert_eq!(repo.branch.as_deref(), Some("next"));
    }

    #[test]
    fn something_that_is_not_a_repository_is_refused_with_the_shape_that_is() {
        let refused = parse_repo("https://example.com").expect_err("should refuse");
        assert!(
            refused.to_string().contains("github.com/owner/name"),
            "the refusal should show what would work: {refused}"
        );
    }

    #[test]
    fn skill_files_are_found_by_name_at_any_depth_and_in_any_case() {
        assert!(is_skill_file("SKILL.md"));
        assert!(is_skill_file("skills/taste/SKILL.md"));
        assert!(is_skill_file("a/b/c/skill.md"));

        // Other files in a skills repository.
        assert!(!is_skill_file("README.md"));
        assert!(!is_skill_file("skills/taste/reference.md"));
        assert!(!is_skill_file("scripts/build.py"));
        assert!(!is_skill_file("MY_SKILL.md"));
    }

    #[test]
    fn shallower_paths_are_preferred_so_a_mirror_never_wins() {
        // Prefer the top-level copy over per-agent mirrors.
        let mut paths = [
            ".cursor/skills/x/SKILL.md".to_string(),
            "SKILL.md".to_string(),
            ".claude/skills/x/SKILL.md".to_string(),
        ];
        paths.sort_by_key(|path| (path.matches('/').count(), path.clone()));

        assert_eq!(paths[0], "SKILL.md");
    }

    #[test]
    fn a_skill_that_expects_scripts_is_noticed() {
        assert!(mentions_scripts("Run `scripts/convert.py` on the file."));
        assert!(!mentions_scripts("Prefer restraint and read the file first."));
    }
}
