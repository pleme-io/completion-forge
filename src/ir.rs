// Intermediate representation for shell completions.

use std::fmt;

use serde::{Deserialize, Serialize};

// ── Top-level IR ───────────────────────────────────────────────────────────

/// A complete completion spec for a single CLI tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionSpec {
    /// CLI command name (e.g., "petstore").
    pub name: String,
    /// Unicode glyph for prompt decoration.
    pub icon: String,
    /// Command aliases (e.g., `["docker", "podman"]`).
    pub aliases: Vec<String>,
    /// Human-readable description.
    pub description: String,
    /// Grouped subcommands.
    pub groups: Vec<CommandGroup>,
}

/// A group of related operations, mapped to a CLI subcommand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandGroup {
    /// Subcommand name (kebab-case).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Category glyph, auto-assigned from HTTP method mix.
    pub glyph: Glyph,
    /// Individual operations within this group.
    pub operations: Vec<CompletionOp>,
    /// Flags available for this subcommand group.
    pub flags: Vec<CompletionFlag>,
}

/// A single completable operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionOp {
    /// Operation name (e.g., "list-pets").
    pub name: String,
    /// Short description.
    pub description: String,
    /// HTTP method that backs this operation.
    pub method: String,
}

/// A completable flag/option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionFlag {
    /// Flag name without dashes (e.g., "limit").
    pub name: String,
    /// Short description.
    pub description: String,
    /// Whether the flag is required.
    pub required: bool,
}

// ── Glyph ─────────────────────────────────────────────────────────────────

/// Category glyph, auto-assigned based on HTTP method mix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Glyph {
    /// Read/inspect operations (all GET).
    View,
    /// Create operations (all POST).
    Create,
    /// Update operations (all PUT/PATCH).
    Update,
    /// Delete operations (all DELETE).
    Delete,
    /// Mixed operations.
    Manage,
    /// Execute/action operations.
    Execute,
    /// Custom glyph string.
    Custom(String),
}

impl Glyph {
    /// Auto-assign glyph from a set of HTTP methods.
    #[must_use]
    pub fn from_methods(methods: &[&str]) -> Self {
        if methods.is_empty() {
            return Self::Manage;
        }

        let all_same = methods.iter().all(|m| *m == methods[0]);
        if all_same {
            return match methods[0] {
                "GET" => Self::View,
                "POST" => Self::Create,
                "PUT" | "PATCH" => Self::Update,
                "DELETE" => Self::Delete,
                _ => Self::Manage,
            };
        }

        Self::Manage
    }

    /// Unicode character for this glyph.
    #[must_use]
    pub fn as_char(&self) -> &str {
        match self {
            Self::View => "\u{25C8}",    // ◈
            Self::Create => "\u{25C7}",  // ◇
            Self::Update => "\u{21BB}",  // ↻
            Self::Delete => "\u{25C7}",  // ◇
            Self::Manage => "\u{2299}",  // ⊙
            Self::Execute => "\u{25B8}", // ▸
            Self::Custom(s) => s,
        }
    }
}

impl fmt::Display for Glyph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_char())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_from_all_get() {
        assert_eq!(Glyph::from_methods(&["GET", "GET"]), Glyph::View);
    }

    #[test]
    fn glyph_from_all_post() {
        assert_eq!(Glyph::from_methods(&["POST"]), Glyph::Create);
    }

    #[test]
    fn glyph_from_all_delete() {
        assert_eq!(Glyph::from_methods(&["DELETE"]), Glyph::Delete);
    }

    #[test]
    fn glyph_from_mixed() {
        assert_eq!(Glyph::from_methods(&["GET", "POST"]), Glyph::Manage);
    }

    #[test]
    fn glyph_from_empty() {
        assert_eq!(Glyph::from_methods(&[]), Glyph::Manage);
    }

    #[test]
    fn glyph_display() {
        assert_eq!(Glyph::View.to_string(), "\u{25C8}");
        assert_eq!(Glyph::Create.to_string(), "\u{25C7}");
        assert_eq!(Glyph::Custom("X".into()).to_string(), "X");
    }

    #[test]
    fn completion_spec_roundtrip() {
        let spec = CompletionSpec {
            name: "test".into(),
            icon: "\u{2601}".into(),
            aliases: vec!["t".into()],
            description: "Test tool".into(),
            groups: vec![CommandGroup {
                name: "pets".into(),
                description: "Pet operations".into(),
                glyph: Glyph::View,
                operations: vec![CompletionOp {
                    name: "list".into(),
                    description: "List pets".into(),
                    method: "GET".into(),
                }],
                flags: vec![CompletionFlag {
                    name: "limit".into(),
                    description: "Max results".into(),
                    required: false,
                }],
            }],
        };

        let yaml = serde_yaml_ng::to_string(&spec).unwrap();
        let parsed: CompletionSpec = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].operations.len(), 1);
    }
}
