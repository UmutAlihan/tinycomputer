//! Agent-facing assets for driving the tinycomputer module.
//!
//! The module is the capability; this crate is the guidance a model needs to
//! use it well — a loadable skill document and the task tool's JSON Schema —
//! versioned with the contract so an installed skill never describes members
//! the module does not serve. [`skill_assets`] lists every file to install.
//!
//! ```
//! use tinycomputer_skills::{SKILL, skill_assets};
//!
//! assert!(SKILL.contains("StartTask"));
//! assert_eq!(skill_assets().len(), 2);
//! ```
//!
//! The crate holds no module, no bus, and no credentials: only text.

/// The loadable skill, in Markdown with front matter.
pub const SKILL: &str = include_str!("../skills/tinycomputer/SKILL.md");

/// JSON Schema for `StartTask`, with the task members it is used alongside.
pub const START_TASK_SCHEMA: &str = include_str!("../schemas/start_task.schema.json");

/// One file a host installs into its skill directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillAsset {
    /// Path relative to the host's skill root.
    pub path: &'static str,
    /// The file's contents.
    pub contents: &'static str,
}

const ASSETS: &[SkillAsset] = &[
    SkillAsset {
        path: "tinycomputer/SKILL.md",
        contents: SKILL,
    },
    SkillAsset {
        path: "tinycomputer/schemas/start_task.schema.json",
        contents: START_TASK_SCHEMA,
    },
];

/// Every file in the skill package.
#[must_use]
pub const fn skill_assets() -> &'static [SkillAsset] {
    ASSETS
}

#[cfg(test)]
mod test;
