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
            Self::Create | Self::Delete => "\u{25C7}", // ◇
            Self::Update => "\u{21BB}",               // ↻
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
    fn glyph_from_all_put() {
        assert_eq!(Glyph::from_methods(&["PUT", "PUT"]), Glyph::Update);
    }

    #[test]
    fn glyph_from_all_patch() {
        assert_eq!(Glyph::from_methods(&["PATCH"]), Glyph::Update);
    }

    #[test]
    fn glyph_from_single_put() {
        assert_eq!(Glyph::from_methods(&["PUT"]), Glyph::Update);
    }

    #[test]
    fn glyph_from_unknown_method() {
        assert_eq!(Glyph::from_methods(&["HEAD"]), Glyph::Manage);
        assert_eq!(Glyph::from_methods(&["OPTIONS"]), Glyph::Manage);
        assert_eq!(Glyph::from_methods(&["TRACE"]), Glyph::Manage);
    }

    #[test]
    fn glyph_from_single_get() {
        assert_eq!(Glyph::from_methods(&["GET"]), Glyph::View);
    }

    #[test]
    fn glyph_from_single_delete() {
        assert_eq!(Glyph::from_methods(&["DELETE"]), Glyph::Delete);
    }

    #[test]
    fn glyph_as_char_all_variants() {
        assert_eq!(Glyph::View.as_char(), "\u{25C8}");
        assert_eq!(Glyph::Create.as_char(), "\u{25C7}");
        assert_eq!(Glyph::Update.as_char(), "\u{21BB}");
        assert_eq!(Glyph::Delete.as_char(), "\u{25C7}");
        assert_eq!(Glyph::Manage.as_char(), "\u{2299}");
        assert_eq!(Glyph::Execute.as_char(), "\u{25B8}");
        assert_eq!(Glyph::Custom("★".into()).as_char(), "★");
    }

    #[test]
    fn glyph_display_all_variants() {
        assert_eq!(Glyph::View.to_string(), "\u{25C8}");
        assert_eq!(Glyph::Create.to_string(), "\u{25C7}");
        assert_eq!(Glyph::Update.to_string(), "\u{21BB}");
        assert_eq!(Glyph::Delete.to_string(), "\u{25C7}");
        assert_eq!(Glyph::Manage.to_string(), "\u{2299}");
        assert_eq!(Glyph::Execute.to_string(), "\u{25B8}");
        assert_eq!(Glyph::Custom("X".into()).to_string(), "X");
    }

    #[test]
    fn glyph_hash_uniqueness() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Glyph::View);
        set.insert(Glyph::Create);
        set.insert(Glyph::Update);
        set.insert(Glyph::Delete);
        set.insert(Glyph::Manage);
        set.insert(Glyph::Execute);
        set.insert(Glyph::Custom("X".into()));
        assert_eq!(set.len(), 7);
        assert!(set.contains(&Glyph::View));
        assert!(set.contains(&Glyph::Execute));
    }

    #[test]
    fn glyph_serde_roundtrip() {
        let glyphs = vec![
            Glyph::View,
            Glyph::Create,
            Glyph::Update,
            Glyph::Delete,
            Glyph::Manage,
            Glyph::Execute,
            Glyph::Custom("★".into()),
        ];
        for g in glyphs {
            let json = serde_json::to_string(&g).unwrap();
            let parsed: Glyph = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, g);
        }
    }

    #[test]
    fn completion_spec_json_roundtrip() {
        let spec = CompletionSpec {
            name: "test".into(),
            icon: "\u{2601}".into(),
            aliases: vec!["t".into()],
            description: "Test tool".into(),
            groups: vec![CommandGroup {
                name: "pets".into(),
                description: "Pet operations".into(),
                glyph: Glyph::Update,
                operations: vec![CompletionOp {
                    name: "update".into(),
                    description: "Update pet".into(),
                    method: "PUT".into(),
                }],
                flags: vec![CompletionFlag {
                    name: "id".into(),
                    description: "Pet ID".into(),
                    required: true,
                }],
            }],
        };

        let json = serde_json::to_string(&spec).unwrap();
        let parsed: CompletionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, spec);
    }

    #[test]
    fn completion_spec_empty_groups() {
        let spec = CompletionSpec {
            name: "empty".into(),
            icon: String::new(),
            aliases: vec![],
            description: String::new(),
            groups: vec![],
        };

        let yaml = serde_yaml_ng::to_string(&spec).unwrap();
        let parsed: CompletionSpec = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(parsed.groups.len(), 0);
        assert_eq!(parsed.name, "empty");
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
