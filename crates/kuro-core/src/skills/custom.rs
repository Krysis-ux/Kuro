//! Skills the user added: uploaded, or pulled out of a repository.
//!
//! Built-ins are a `const` array with `&'static str` fields, and many call sites
//! take `&'static Skill`. Rather than make `Skill` owned, a user skill is leaked
//! into `'static` when loaded. The leak is bounded (a few kilobytes per skill,
//! reloaded only when the user adds or removes one, in a desktop daemon) but
//! would be a bug if done per request.

use std::sync::RwLock;

use crate::db::{Db, UserSkillRecord};
use crate::Result;

use super::{Skill, SkillCategory};

/// The loaded user skills, in the shape everything downstream already reads.
static CUSTOM: RwLock<Vec<&'static Skill>> = RwLock::new(Vec::new());

/// Rough token count (4 characters per token), only used to warn that several
/// skills at once are expensive.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// What a `SKILL.md` turned out to contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkill {
    pub slug: String,
    pub name: String,
    pub blurb: String,
    pub category: SkillCategory,
    pub instructions: String,
}

/// Turn a name into something typeable after a slash.
pub fn slugify(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_dash = true;

    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Read a `SKILL.md`: YAML front matter (`name`, `description`) then markdown
/// instructions. Parsed by hand since only `key: value` lines are used; a YAML
/// parser would accept constructs this cannot represent.
///
/// Without front matter, the first heading becomes the name and the first
/// paragraph the description.
pub fn parse_skill_md(content: &str, fallback_name: &str) -> Result<ParsedSkill> {
    let (front, body) = split_front_matter(content);

    let mut name = field(&front, "name");
    let mut blurb = field(&front, "description").or_else(|| field(&front, "blurb"));
    let category = field(&front, "category")
        .and_then(|raw| category_from(&raw))
        .unwrap_or(SkillCategory::Practice);

    if name.is_none() {
        name = body
            .lines()
            .find(|line| line.starts_with('#'))
            .map(|line| line.trim_start_matches('#').trim().to_string())
            .filter(|found| !found.is_empty());
    }

    if blurb.is_none() {
        blurb = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| line.chars().take(160).collect());
    }

    let name = name.unwrap_or_else(|| fallback_name.to_string());
    let slug = slugify(&name);
    if slug.is_empty() {
        return Err(crate::KuroError::bad_request(
            "this file has no name to call the skill by. Give it a `name:` in the front \
             matter, or a heading.",
        ));
    }

    let instructions = body.trim();
    if instructions.is_empty() {
        return Err(crate::KuroError::bad_request(format!(
            "`{name}` has front matter but no instructions under it, so there would be \
             nothing to add to the prompt."
        )));
    }

    let instructions: String = instructions
        .chars()
        .take(crate::db::MAX_USER_SKILL_CHARS)
        .collect();

    Ok(ParsedSkill {
        blurb: blurb.unwrap_or_else(|| format!("Added from {fallback_name}")),
        name,
        slug,
        category,
        instructions,
    })
}

/// Split `---` front matter from the body. Absent front matter is not an error.
fn split_front_matter(content: &str) -> (String, String) {
    let trimmed = content.trim_start_matches('\u{feff}').trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (String::new(), content.to_string());
    };
    let rest = rest.trim_start_matches(['\r', '\n']);

    // The closing fence must be on its own line so a `---` rule in the body does
    // not truncate it.
    match rest.split_once("\n---") {
        Some((front, body)) => (
            front.to_string(),
            body.trim_start_matches(['-', '\r', '\n']).to_string(),
        ),
        None => (String::new(), content.to_string()),
    }
}

/// One `key: value` out of the front matter.
fn field(front: &str, key: &str) -> Option<String> {
    front.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case(key) {
            return None;
        }
        let value = value.trim().trim_matches(['"', '\'']).trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn category_from(raw: &str) -> Option<SkillCategory> {
    match raw.to_ascii_lowercase().as_str() {
        "language" | "languages" => Some(SkillCategory::Language),
        "coding" | "code" => Some(SkillCategory::Coding),
        "practice" | "engineering" => Some(SkillCategory::Practice),
        "design" | "ui" | "frontend" => Some(SkillCategory::Design),
        "writing" | "reasoning" => Some(SkillCategory::Writing),
        _ => None,
    }
}

/// Every user skill currently loaded.
pub fn loaded() -> Vec<&'static Skill> {
    CUSTOM.read().expect("custom skills lock").clone()
}

/// One user skill by slug.
pub fn find(slug: &str) -> Option<&'static Skill> {
    CUSTOM
        .read()
        .expect("custom skills lock")
        .iter()
        .copied()
        .find(|skill| skill.slug == slug)
}

/// Load the user's skills from storage, replacing the whole list. Called at
/// startup and after anything that changes the table; the only path by which a
/// user skill becomes visible.
pub fn reload(db: &Db) -> Result<usize> {
    let records = db.list_user_skills()?;
    let loaded: Vec<&'static Skill> = records.iter().map(leak).collect();
    let count = loaded.len();
    *CUSTOM.write().expect("custom skills lock") = loaded;
    Ok(count)
}

fn leak(record: &UserSkillRecord) -> &'static Skill {
    Box::leak(Box::new(Skill {
        slug: Box::leak(record.slug.clone().into_boxed_str()),
        name: Box::leak(record.name.clone().into_boxed_str()),
        blurb: Box::leak(record.blurb.clone().into_boxed_str()),
        category: category_from(&record.category).unwrap_or(SkillCategory::Practice),
        instructions: Box::leak(record.instructions.clone().into_boxed_str()),
        approx_tokens: record.approx_tokens,
        // Never essential: that would let a downloaded file force itself on in
        // every coding workspace.
        essential: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_gives_the_name_and_the_description() {
        let parsed = parse_skill_md(
            "---\nname: Taste\ndescription: Make interfaces feel considered.\n---\n\nPrefer \
             restraint.\n",
            "taste.md",
        )
        .expect("parsed");

        assert_eq!(parsed.name, "Taste");
        assert_eq!(parsed.slug, "taste");
        assert_eq!(parsed.blurb, "Make interfaces feel considered.");
        assert_eq!(parsed.instructions, "Prefer restraint.");
    }

    #[test]
    fn a_file_without_front_matter_is_still_a_skill() {
        // The heading stands in for the front matter.
        let parsed = parse_skill_md("# Impeccable\n\nShip nothing half-done.\n", "impeccable.md")
            .expect("parsed");

        assert_eq!(parsed.name, "Impeccable");
        assert_eq!(parsed.slug, "impeccable");
        assert_eq!(parsed.blurb, "Ship nothing half-done.");
        assert!(parsed.instructions.contains("Ship nothing half-done."));
    }

    #[test]
    fn a_rule_in_the_body_does_not_truncate_the_instructions() {
        // `---` is also a horizontal rule and must not end the body.
        let parsed = parse_skill_md(
            "---\nname: Sections\n---\n\nFirst part.\n\n---\n\nSecond part.\n",
            "sections.md",
        )
        .expect("parsed");

        assert!(parsed.instructions.contains("First part."));
        assert!(
            parsed.instructions.contains("Second part."),
            "everything after the horizontal rule was dropped"
        );
    }

    #[test]
    fn a_category_is_read_when_given_and_guessed_safely_when_not() {
        let given = parse_skill_md("---\nname: A\ncategory: design\n---\nDo it.", "a.md")
            .expect("parsed");
        assert_eq!(given.category, SkillCategory::Design);

        let absent = parse_skill_md("---\nname: B\n---\nDo it.", "b.md").expect("parsed");
        assert_eq!(absent.category, SkillCategory::Practice);
    }

    #[test]
    fn a_file_with_no_instructions_is_refused_rather_than_stored_empty() {
        let refused = parse_skill_md("---\nname: Empty\ndescription: nothing\n---\n\n", "e.md");
        assert!(refused.is_err(), "an empty skill adds nothing to a prompt");
    }

    #[test]
    fn names_become_slugs_that_can_be_typed_after_a_slash() {
        assert_eq!(slugify("Next.js best practices"), "next-js-best-practices");
        assert_eq!(slugify("  Taste  "), "taste");
        assert_eq!(slugify("C++"), "c");
        assert_eq!(slugify("!!!"), "");
    }
}
