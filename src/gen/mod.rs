// Generator dispatcher — routes to format-specific generators.

/// Fish shell completion generator.
pub mod fish;
/// Skim-tab YAML completion generator.
pub mod skim_tab;

use std::fmt;
use std::path::Path;

use anyhow::{Context, Result};

use crate::ir::CompletionSpec;

/// Output format for generated completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Skim-tab YAML format.
    SkimTab,
    /// Fish shell completion format.
    Fish,
    /// Generate all supported formats.
    All,
}

impl Format {
    /// Parse from string (for CLI).
    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "skim-tab" | "skim_tab" | "skimtab" | "yaml" => Self::SkimTab,
            "fish" => Self::Fish,
            _ => Self::All,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SkimTab => write!(f, "skim-tab"),
            Self::Fish => write!(f, "fish"),
            Self::All => write!(f, "all"),
        }
    }
}

/// Trait for format-specific output generators.
pub trait OutputGenerator {
    /// Name of this output format.
    fn format_name(&self) -> &'static str;
    /// Generate completion files and return the output path.
    ///
    /// # Errors
    /// Returns an error if file I/O or serialization fails.
    fn generate(&self, spec: &CompletionSpec, output_dir: &Path) -> Result<String>;
}

/// Generator for skim-tab YAML output.
pub struct SkimTabGenerator;

impl OutputGenerator for SkimTabGenerator {
    fn format_name(&self) -> &'static str {
        "skim-tab"
    }

    fn generate(&self, spec: &CompletionSpec, output_dir: &Path) -> Result<String> {
        skim_tab::generate(spec, output_dir)
    }
}

/// Generator for fish shell completion output.
pub struct FishGenerator;

impl OutputGenerator for FishGenerator {
    fn format_name(&self) -> &'static str {
        "fish"
    }

    fn generate(&self, spec: &CompletionSpec, output_dir: &Path) -> Result<String> {
        fish::generate(spec, output_dir)
    }
}

/// Generate completion files in the specified format.
///
/// # Errors
/// Returns an error if file I/O fails.
pub fn generate(spec: &CompletionSpec, output_dir: &Path, format: Format) -> Result<Vec<String>> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create output directory: {}", output_dir.display()))?;

    let generators: Vec<&dyn OutputGenerator> = match format {
        Format::SkimTab => vec![&SkimTabGenerator],
        Format::Fish => vec![&FishGenerator],
        Format::All => vec![&SkimTabGenerator, &FishGenerator],
    };

    let mut generated = Vec::new();
    for generator in generators {
        let path = generator
            .generate(spec, output_dir)
            .with_context(|| {
                format!("failed to generate {} completions", generator.format_name())
            })?;
        generated.push(path);
    }

    Ok(generated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_parsing() {
        assert_eq!(Format::from_str_loose("skim-tab"), Format::SkimTab);
        assert_eq!(Format::from_str_loose("yaml"), Format::SkimTab);
        assert_eq!(Format::from_str_loose("fish"), Format::Fish);
        assert_eq!(Format::from_str_loose("all"), Format::All);
        assert_eq!(Format::from_str_loose("unknown"), Format::All);
    }

    #[test]
    fn format_display() {
        assert_eq!(Format::SkimTab.to_string(), "skim-tab");
        assert_eq!(Format::Fish.to_string(), "fish");
        assert_eq!(Format::All.to_string(), "all");
    }

    struct MockGenerator;
    impl OutputGenerator for MockGenerator {
        fn format_name(&self) -> &'static str {
            "mock"
        }
        fn generate(&self, _spec: &CompletionSpec, _output: &Path) -> Result<String> {
            Ok("mock.txt".to_owned())
        }
    }

    #[test]
    fn mock_generator_works() {
        let mock: &dyn OutputGenerator = &MockGenerator;
        assert_eq!(mock.format_name(), "mock");
    }

    fn sample_spec() -> CompletionSpec {
        use crate::ir::{CommandGroup, CompletionFlag, CompletionOp, Glyph};
        CompletionSpec {
            name: "test-tool".into(),
            icon: "☁".into(),
            aliases: vec!["tt".into()],
            description: "Test tool".into(),
            groups: vec![CommandGroup {
                name: "items".into(),
                description: "Item operations".into(),
                glyph: Glyph::View,
                operations: vec![CompletionOp {
                    name: "list".into(),
                    description: "List items".into(),
                    method: "GET".into(),
                }],
                flags: vec![CompletionFlag {
                    name: "limit".into(),
                    description: "Max results".into(),
                    required: false,
                }],
            }],
        }
    }

    #[test]
    fn generate_skim_tab_format() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let result = generate(&spec, dir.path(), Format::SkimTab).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with(".yaml"));
    }

    #[test]
    fn generate_fish_format() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let result = generate(&spec, dir.path(), Format::Fish).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with(".fish"));
    }

    #[test]
    fn generate_all_format() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let result = generate(&spec, dir.path(), Format::All).unwrap();
        assert_eq!(result.len(), 2);
        let extensions: Vec<&str> = result
            .iter()
            .map(|p| {
                if p.ends_with(".yaml") {
                    "yaml"
                } else if p.ends_with(".fish") {
                    "fish"
                } else {
                    "other"
                }
            })
            .collect();
        assert!(extensions.contains(&"yaml"));
        assert!(extensions.contains(&"fish"));
    }

    #[test]
    fn generate_creates_output_dir() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        assert!(!nested.exists());

        let spec = sample_spec();
        let result = generate(&spec, &nested, Format::SkimTab).unwrap();
        assert!(nested.exists());
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn generate_overwrites_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();

        let result1 = generate(&spec, dir.path(), Format::SkimTab).unwrap();
        let content1 = std::fs::read_to_string(&result1[0]).unwrap();

        let result2 = generate(&spec, dir.path(), Format::SkimTab).unwrap();
        let content2 = std::fs::read_to_string(&result2[0]).unwrap();

        assert_eq!(content1, content2);
    }

    #[test]
    fn skim_tab_generator_format_name() {
        let skim = SkimTabGenerator;
        assert_eq!(skim.format_name(), "skim-tab");
    }

    #[test]
    fn fish_generator_format_name() {
        let fish = FishGenerator;
        assert_eq!(fish.format_name(), "fish");
    }

    #[test]
    fn skim_tab_generator_through_trait() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let generator: &dyn OutputGenerator = &SkimTabGenerator;
        let result = generator.generate(&spec, dir.path()).unwrap();
        assert!(result.ends_with(".yaml"));
    }

    #[test]
    fn fish_generator_through_trait() {
        let dir = tempfile::tempdir().unwrap();
        let spec = sample_spec();
        let generator: &dyn OutputGenerator = &FishGenerator;
        let result = generator.generate(&spec, dir.path()).unwrap();
        assert!(result.ends_with(".fish"));
    }

    #[test]
    fn format_from_str_additional_aliases() {
        assert_eq!(Format::from_str_loose("skim_tab"), Format::SkimTab);
        assert_eq!(Format::from_str_loose("skimtab"), Format::SkimTab);
        assert_eq!(Format::from_str_loose("FISH"), Format::Fish);
        assert_eq!(Format::from_str_loose("ALL"), Format::All);
        assert_eq!(Format::from_str_loose("Skim-Tab"), Format::SkimTab);
    }

    struct FailingGenerator;
    impl OutputGenerator for FailingGenerator {
        fn format_name(&self) -> &'static str {
            "failing"
        }
        fn generate(&self, _spec: &CompletionSpec, _output: &Path) -> Result<String> {
            anyhow::bail!("intentional failure")
        }
    }

    #[test]
    fn failing_generator_returns_error() {
        let spec = sample_spec();
        let dir = tempfile::tempdir().unwrap();
        let generator: &dyn OutputGenerator = &FailingGenerator;
        let result = generator.generate(&spec, dir.path());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("intentional failure"));
    }

    #[test]
    fn format_eq_and_copy() {
        let a = Format::SkimTab;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(Format::Fish, Format::All);
    }

    #[test]
    fn format_debug() {
        let debug = format!("{:?}", Format::SkimTab);
        assert!(debug.contains("SkimTab"));
    }

    #[test]
    fn generate_empty_spec() {
        let dir = tempfile::tempdir().unwrap();
        let spec = CompletionSpec {
            name: "empty".into(),
            icon: String::new(),
            aliases: vec![],
            description: "Empty".into(),
            groups: vec![],
        };
        let result = generate(&spec, dir.path(), Format::All).unwrap();
        assert_eq!(result.len(), 2);
    }
}
