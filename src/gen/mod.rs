// Generator dispatcher — routes to format-specific generators.

pub mod fish;
pub mod skim_tab;

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

/// Generate completion files in the specified format.
///
/// # Errors
/// Returns an error if file I/O fails.
pub fn generate(spec: &CompletionSpec, output_dir: &Path, format: Format) -> Result<Vec<String>> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create output directory: {}", output_dir.display()))?;

    let mut generated = Vec::new();

    if matches!(format, Format::SkimTab | Format::All) {
        let path = skim_tab::generate(spec, output_dir)
            .context("failed to generate skim-tab YAML")?;
        generated.push(path);
    }

    if matches!(format, Format::Fish | Format::All) {
        let path = fish::generate(spec, output_dir).context("failed to generate fish completions")?;
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
}
