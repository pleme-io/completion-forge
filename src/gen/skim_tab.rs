// Generate skim-tab YAML completion specs.
//
// Output format matches skim-tab's `CompletionSpec` serde structure:
// ```yaml
// commands: [tool-name, alias1]
// icon: "☁"
// subcommands:
//   subcommand-name:
//     description: "Human description"
//     glyph: "◈"
// ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::error::{ForgeError, ForgeResult};
use crate::ir::CompletionSpec;

// ── Output types matching skim-tab's serde format ─────────────────────────

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
struct SkimTabSpec {
    commands: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    icon: String,
    subcommands: BTreeMap<String, SkimTabSubcommand>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
struct SkimTabSubcommand {
    description: String,
    glyph: String,
}

// ── Generator ─────────────────────────────────────────────────────────────

/// Generate a skim-tab YAML file and return its path.
///
/// # Errors
/// Returns [`ForgeError::Yaml`] if serialization fails, or
/// [`ForgeError::Io`] if the file cannot be written.
pub fn generate(spec: &CompletionSpec, output_dir: &Path) -> ForgeResult<String> {
    let mut commands = vec![spec.name.clone()];
    commands.extend(spec.aliases.iter().cloned());

    let subcommands = spec
        .groups
        .iter()
        .map(|group| {
            (
                group.name.clone(),
                SkimTabSubcommand {
                    description: group.description.clone(),
                    glyph: group.glyph.as_char().to_owned(),
                },
            )
        })
        .collect();

    let output = SkimTabSpec {
        commands,
        icon: spec.icon.clone(),
        subcommands,
    };

    let yaml = serde_yaml_ng::to_string(&output)?;
    let filename = format!("{}.yaml", spec.name);
    let path = output_dir.join(&filename);
    std::fs::write(&path, &yaml).map_err(|e| ForgeError::io(&path, e))?;

    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CommandGroup, CompletionOp, Glyph};

    fn sample_spec() -> CompletionSpec {
        CompletionSpec {
            name: "petstore".into(),
            icon: "\u{2601}".into(),
            aliases: vec!["ps".into()],
            description: "Pet store API".into(),
            groups: vec![
                CommandGroup {
                    name: "pets".into(),
                    description: "Pet operations".into(),
                    glyph: Glyph::View,
                    operations: vec![CompletionOp {
                        name: "list-pets".into(),
                        description: "List all pets".into(),
                        method: "GET".into(),
                    }],
                    flags: vec![],
                },
                CommandGroup {
                    name: "stores".into(),
                    description: "Store operations".into(),
                    glyph: Glyph::Manage,
                    operations: vec![],
                    flags: vec![],
                },
            ],
        }
    }

    #[test]
    fn generate_yaml_file() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let path = generate(&spec, dir.path()).unwrap();
        assert!(path.ends_with("petstore.yaml"));

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("commands:"));
        assert!(content.contains("petstore"));
        assert!(content.contains("ps"));
        assert!(content.contains("pets:"));
        assert!(content.contains("stores:"));
    }

    #[test]
    fn roundtrip_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let path = generate(&spec, dir.path()).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SkimTabSpec = serde_yaml_ng::from_str(&content).unwrap();
        assert_eq!(parsed.commands, vec!["petstore", "ps"]);
        assert_eq!(parsed.subcommands.len(), 2);
        assert!(parsed.subcommands.contains_key("pets"));
        assert!(parsed.subcommands.contains_key("stores"));
    }

    #[test]
    fn yaml_glyphs_correct() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let path = generate(&spec, dir.path()).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SkimTabSpec = serde_yaml_ng::from_str(&content).unwrap();
        assert_eq!(parsed.subcommands["pets"].glyph, "\u{25C8}"); // View
        assert_eq!(parsed.subcommands["stores"].glyph, "\u{2299}"); // Manage
    }

    #[test]
    fn empty_icon_omitted() {
        let dir = tempfile::tempdir().unwrap();
        let spec = CompletionSpec {
            name: "test".into(),
            icon: String::new(),
            aliases: vec![],
            description: "Test".into(),
            groups: vec![],
        };
        let path = generate(&spec, dir.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("icon:"));
    }

    #[test]
    fn generate_no_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let spec = CompletionSpec {
            name: "solo".into(),
            icon: "★".into(),
            aliases: vec![],
            description: "Solo tool".into(),
            groups: vec![CommandGroup {
                name: "items".into(),
                description: "Item operations".into(),
                glyph: Glyph::Create,
                operations: vec![],
                flags: vec![],
            }],
        };

        let path = generate(&spec, dir.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SkimTabSpec = serde_yaml_ng::from_str(&content).unwrap();
        assert_eq!(parsed.commands, vec!["solo"]);
        assert_eq!(parsed.icon, "★");
    }

    #[test]
    fn generate_all_glyph_types() {
        use crate::ir::Glyph;

        let dir = tempfile::tempdir().unwrap();
        let glyphs = vec![
            ("view-group", Glyph::View, "\u{25C8}"),
            ("create-group", Glyph::Create, "\u{25C7}"),
            ("update-group", Glyph::Update, "\u{21BB}"),
            ("delete-group", Glyph::Delete, "\u{25C7}"),
            ("manage-group", Glyph::Manage, "\u{2299}"),
            ("execute-group", Glyph::Execute, "\u{25B8}"),
            ("custom-group", Glyph::Custom("✦".into()), "✦"),
        ];

        let groups: Vec<CommandGroup> = glyphs
            .iter()
            .map(|(name, glyph, _)| CommandGroup {
                name: (*name).into(),
                description: format!("{name} desc"),
                glyph: glyph.clone(),
                operations: vec![],
                flags: vec![],
            })
            .collect();

        let spec = CompletionSpec {
            name: "glyph-test".into(),
            icon: "☁".into(),
            aliases: vec![],
            description: "Glyph test".into(),
            groups,
        };

        let path = generate(&spec, dir.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SkimTabSpec = serde_yaml_ng::from_str(&content).unwrap();

        for (name, _, expected_char) in &glyphs {
            let sub = &parsed.subcommands[*name];
            assert_eq!(
                sub.glyph, *expected_char,
                "glyph mismatch for {name}"
            );
        }
    }

    #[test]
    fn generate_multiple_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let spec = CompletionSpec {
            name: "main-tool".into(),
            icon: "⚡".into(),
            aliases: vec!["mt".into(), "tool".into(), "m".into()],
            description: "Multi alias tool".into(),
            groups: vec![],
        };

        let path = generate(&spec, dir.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SkimTabSpec = serde_yaml_ng::from_str(&content).unwrap();
        assert_eq!(parsed.commands, vec!["main-tool", "mt", "tool", "m"]);
    }

    #[test]
    fn generate_empty_groups() {
        let dir = tempfile::tempdir().unwrap();
        let spec = CompletionSpec {
            name: "bare".into(),
            icon: String::new(),
            aliases: vec![],
            description: "Bare tool".into(),
            groups: vec![],
        };

        let path = generate(&spec, dir.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("commands:"));
        assert!(!content.contains("icon:"), "empty icon should be omitted");
        assert!(content.contains("subcommands: {}"));
    }

    #[test]
    fn test_generate_with_many_groups() {
        use crate::ir::CompletionFlag;

        let groups: Vec<CommandGroup> = (0..12)
            .map(|i| {
                let glyph = if i % 2 == 0 { Glyph::View } else { Glyph::Create };
                CommandGroup {
                    name: format!("group-{i}"),
                    description: format!("Group {i} operations"),
                    glyph,
                    operations: vec![CompletionOp {
                        name: format!("op-{i}"),
                        description: format!("Operation {i}"),
                        method: "GET".into(),
                    }],
                    flags: vec![CompletionFlag {
                        name: format!("flag-{i}"),
                        description: format!("Flag {i}"),
                        required: false,
                    }],
                }
            })
            .collect();

        let spec = CompletionSpec {
            name: "big-api".into(),
            icon: "\u{2601}".into(),
            aliases: vec![],
            description: "API with many groups".into(),
            groups,
        };

        let dir = tempfile::tempdir().unwrap();
        let path = generate(&spec, dir.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SkimTabSpec = serde_yaml_ng::from_str(&content).unwrap();

        assert_eq!(parsed.subcommands.len(), 12);
        for i in 0..12 {
            let key = format!("group-{i}");
            assert!(
                parsed.subcommands.contains_key(&key),
                "missing subcommand: {key}"
            );
        }
    }

    #[test]
    fn yaml_subcommand_descriptions_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let spec = CompletionSpec {
            name: "desc-test".into(),
            icon: "★".into(),
            aliases: vec![],
            description: "Test".into(),
            groups: vec![CommandGroup {
                name: "things".into(),
                description: "Manage all the things".into(),
                glyph: Glyph::Manage,
                operations: vec![],
                flags: vec![],
            }],
        };
        let path = generate(&spec, dir.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SkimTabSpec = serde_yaml_ng::from_str(&content).unwrap();
        assert_eq!(
            parsed.subcommands["things"].description,
            "Manage all the things"
        );
    }

    #[test]
    fn yaml_output_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let path1 = generate(&spec, dir.path()).unwrap();
        let content1 = std::fs::read_to_string(&path1).unwrap();
        let path2 = generate(&spec, dir.path()).unwrap();
        let content2 = std::fs::read_to_string(&path2).unwrap();
        assert_eq!(content1, content2, "YAML output should be deterministic");
    }

    #[test]
    fn yaml_subcommands_sorted_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let spec = CompletionSpec {
            name: "sorted".into(),
            icon: String::new(),
            aliases: vec![],
            description: "Sort test".into(),
            groups: vec![
                CommandGroup {
                    name: "zebra".into(),
                    description: "Z ops".into(),
                    glyph: Glyph::View,
                    operations: vec![],
                    flags: vec![],
                },
                CommandGroup {
                    name: "alpha".into(),
                    description: "A ops".into(),
                    glyph: Glyph::Create,
                    operations: vec![],
                    flags: vec![],
                },
            ],
        };
        let path = generate(&spec, dir.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let alpha_pos = content.find("alpha:").unwrap();
        let zebra_pos = content.find("zebra:").unwrap();
        assert!(
            alpha_pos < zebra_pos,
            "subcommands should be sorted alphabetically (BTreeMap)"
        );
    }
}
