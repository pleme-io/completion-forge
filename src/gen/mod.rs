// Generator dispatcher — routes to format-specific generators.

pub mod fish;
pub mod skim_tab;

use std::fmt;
use std::path::Path;

use anyhow::{Context, Result};

use crate::ir::CompletionSpec;

/// Output format for generated completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    SkimTab,
    Fish,
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
}
