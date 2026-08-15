//! Coding workspaces: a folder, a mode, and the tools that mode allows.
//!
//! Kuro's chat surface deliberately cannot touch the filesystem. Everything that
//! reads or changes a file lives here instead, behind a workspace: a folder the
//! user chose, and a mode saying how much the model may do inside it.
//!
//! ## Why the mode is the permission
//!
//! A model asking to write a file is not a question that can be answered well
//! mid-generation. The user is watching a reply arrive; interrupting it with a
//! dialog they did not expect, about a path they have not seen, is how people
//! learn to click Allow without reading. So the decision is moved earlier and
//! made once, in a place where the whole answer is "what may this do":
//!
//! * [`WorkspaceMode::Ask`] — no tools. The model discusses code and cannot see
//!   the project at all.
//! * [`WorkspaceMode::Plan`] — reading only. It can look at the project, search
//!   it, and propose changes it is not able to make.
//! * [`WorkspaceMode::Agent`] — reading, writing, and running the ordinary
//!   development commands, inside the workspace root.
//! * [`WorkspaceMode::Bypass`] — the same, with no command allowlist. The mode
//!   for someone who has decided to stop being asked.
//!
//! The mode is chosen before the turn, shown while it runs, and every change it
//! makes is recorded with the previous contents so it can be undone. That is a
//! weaker guarantee than a per-call prompt and a much stronger one than a switch
//! called "files" that grants a model the whole home directory.
//!
//! Containment is separate from permission and is not negotiable: a path is
//! resolved, symlinks followed and `..` collapsed, before it is checked against
//! the root, and credentials are refused wherever they appear inside it. See
//! [`crate::tools::files`].

pub mod exec;
pub mod process;
pub mod search;
pub mod tools;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::tools::files::{FileAccess, FilePermissions};

pub use process::{ProcessRegistry, RunningProcess};
pub use tools::{CodingTool, WorkspaceContext};

/// How much a model may do in a workspace this turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    /// Talk about code. No tools, no access to the project.
    Ask,
    /// Read and search the project. Propose changes without making them.
    #[default]
    Plan,
    /// Read, search, change files, and run ordinary development commands.
    Agent,
    /// Agent without the command allowlist.
    Bypass,
}

impl WorkspaceMode {
    pub const ALL: &'static [WorkspaceMode] =
        &[Self::Ask, Self::Plan, Self::Agent, Self::Bypass];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Plan => "plan",
            Self::Agent => "agent",
            Self::Bypass => "bypass",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ask" => Some(Self::Ask),
            "plan" => Some(Self::Plan),
            "agent" | "auto" => Some(Self::Agent),
            "bypass" | "yolo" => Some(Self::Bypass),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::Plan => "Plan",
            Self::Agent => "Agent",
            Self::Bypass => "Bypass",
        }
    }

    /// One line, shown next to the switch. Written so the difference between the
    /// modes is legible without reading documentation.
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Ask => "Discuss code. The model cannot see your project.",
            Self::Plan => "Read and search the project. It cannot change anything.",
            Self::Agent => {
                "Read and change files, and run build, test and dev commands. Every edit can \
                 be undone."
            }
            Self::Bypass => {
                "Everything Agent does, with no command allowlist. Only use this on a project \
                 you could throw away."
            }
        }
    }

    /// Whether this mode permits a tool of the given risk.
    pub fn allows(self, risk: ToolRisk) -> bool {
        match self {
            Self::Ask => false,
            Self::Plan => risk == ToolRisk::Read,
            Self::Agent | Self::Bypass => true,
        }
    }

    /// Whether a command has to be on the allowlist before it will run.
    ///
    /// The one thing Bypass actually changes. Everything else — the working
    /// directory, the containment of file paths, the always-refused commands —
    /// is identical, because those are not permissions the user is turning off.
    pub fn restricts_commands(self) -> bool {
        self != Self::Bypass
    }

    /// The file access tier this mode maps onto.
    pub fn file_access(self) -> FileAccess {
        match self {
            Self::Ask => FileAccess::Off,
            Self::Plan => FileAccess::Read,
            Self::Agent | Self::Bypass => FileAccess::Write,
        }
    }
}

/// What a tool does to the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    /// Looks at the project and changes nothing.
    Read,
    /// Creates or modifies a file inside the workspace root. Recorded, and
    /// reversible from the changes panel.
    Write,
    /// Runs a program. The working directory is the workspace root, but a
    /// command is not confined the way a path is: anything it does afterwards is
    /// the program's own business. This is the one tier that cannot be undone.
    Execute,
}

impl ToolRisk {
    pub fn label(self) -> &'static str {
        match self {
            Self::Read => "Reads",
            Self::Write => "Changes files",
            Self::Execute => "Runs commands",
        }
    }
}

/// A workspace as the tool layer needs it: where it is, and what is allowed.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: String,
    pub root: PathBuf,
    pub mode: WorkspaceMode,
}

impl Workspace {
    /// Containment rules for this workspace.
    ///
    /// Built from the root and the mode rather than from a stored setting, so
    /// there is no way for a workspace to reach a folder that is not its own,
    /// and no way for the tier to disagree with the mode the user can see.
    pub fn permissions(&self) -> FilePermissions {
        FilePermissions {
            access: self.mode.file_access(),
            roots: vec![self.root.clone()],
        }
    }

    /// Whether the root still exists. A folder that was moved or deleted leaves
    /// the workspace in the list — the record is still the user's — but nothing
    /// in it can run.
    pub fn root_exists(&self) -> bool {
        self.root.is_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_round_trip() {
        for mode in WorkspaceMode::ALL {
            assert_eq!(WorkspaceMode::parse(mode.as_str()), Some(*mode));
        }
        assert_eq!(WorkspaceMode::parse("  AGENT "), Some(WorkspaceMode::Agent));
        assert_eq!(WorkspaceMode::parse("root"), None);
    }

    #[test]
    fn ask_mode_allows_nothing_at_all() {
        for risk in [ToolRisk::Read, ToolRisk::Write, ToolRisk::Execute] {
            assert!(!WorkspaceMode::Ask.allows(risk));
        }
        assert_eq!(WorkspaceMode::Ask.file_access(), FileAccess::Off);
    }

    #[test]
    fn plan_mode_can_read_but_never_write_or_run() {
        assert!(WorkspaceMode::Plan.allows(ToolRisk::Read));
        assert!(
            !WorkspaceMode::Plan.allows(ToolRisk::Write),
            "the whole point of planning is that it cannot change anything"
        );
        assert!(
            !WorkspaceMode::Plan.allows(ToolRisk::Execute),
            "a command can change the project without going through a file tool"
        );
        assert!(!WorkspaceMode::Plan.file_access().allows_write());
    }

    #[test]
    fn agent_mode_can_write_and_run() {
        assert!(WorkspaceMode::Agent.allows(ToolRisk::Read));
        assert!(WorkspaceMode::Agent.allows(ToolRisk::Write));
        assert!(WorkspaceMode::Agent.allows(ToolRisk::Execute));
        assert!(WorkspaceMode::Agent.file_access().allows_write());
    }

    #[test]
    fn bypass_differs_from_agent_only_in_the_command_allowlist() {
        for risk in [ToolRisk::Read, ToolRisk::Write, ToolRisk::Execute] {
            assert_eq!(
                WorkspaceMode::Agent.allows(risk),
                WorkspaceMode::Bypass.allows(risk)
            );
        }
        assert_eq!(
            WorkspaceMode::Agent.file_access(),
            WorkspaceMode::Bypass.file_access()
        );
        assert!(WorkspaceMode::Agent.restricts_commands());
        assert!(!WorkspaceMode::Bypass.restricts_commands());
    }

    #[test]
    fn the_names_other_tools_use_for_these_modes_are_accepted() {
        assert_eq!(WorkspaceMode::parse("auto"), Some(WorkspaceMode::Agent));
        assert_eq!(WorkspaceMode::parse("yolo"), Some(WorkspaceMode::Bypass));
    }

    #[test]
    fn permissions_are_confined_to_the_workspace_root() {
        let root = std::env::temp_dir().join(format!("kuro-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::write(root.join("src/main.rs"), "fn main() {}").expect("write");

        let workspace = Workspace {
            id: "w1".to_string(),
            root: root.clone(),
            mode: WorkspaceMode::Agent,
        };
        let permissions = workspace.permissions();

        assert!(permissions.resolve_path("src/main.rs", false).is_ok());
        // The classic escape, and the reason paths are canonicalised before the
        // root check rather than string-matched.
        assert!(permissions.resolve_path("../../../etc/hosts", false).is_err());
        assert!(permissions.resolve_path("/etc/hosts", false).is_err());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn planning_refuses_a_write_even_inside_the_root() {
        let root = std::env::temp_dir().join(format!("kuro-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");

        let planning = Workspace {
            id: "w1".to_string(),
            root: root.clone(),
            mode: WorkspaceMode::Plan,
        };

        assert!(planning.permissions().resolve_path("new.txt", true).is_err());

        std::fs::remove_dir_all(&root).ok();
    }
}
