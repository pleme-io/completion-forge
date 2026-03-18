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

use anyhow::{Context, Result};
use serde::Serialize;

use crate::ir::CompletionSpec;

// ── Output types matching skim-tab's serde format ─────────────────────────

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
struct SkimTabSpec {
    commands: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
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
/// Returns an error if file I/O or serialization fails.
pub fn generate(spec: &CompletionSpec, output_dir: &Path) -> Result<String> {
    let mut commands = vec![spec.name.clone()];
    commands.extend(spec.aliases.iter().cloned());

    let mut subcommands = BTreeMap::new();
    for group in &spec.groups {
        subcommands.insert(
            group.name.clone(),
            SkimTabSubcommand {
                description: group.description.clone(),
                glyph: group.glyph.as_char().to_owned(),
            },
        );
    }

    let output = SkimTabSpec {
        commands,
        icon: spec.icon.clone(),
        subcommands,
    };

    let yaml = serde_yaml_ng::to_string(&output).context("failed to serialize YAML")?;
    let filename = format!("{}.yaml", spec.name);
    let path = output_dir.join(&filename);
    std::fs::write(&path, &yaml)
        .with_context(|| format!("failed to write {}", path.display()))?;

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
}
