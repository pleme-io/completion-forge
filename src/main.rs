use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub mod convert;
pub mod r#gen;
pub mod ir;
pub mod spec;

#[derive(Parser)]
#[command(
    name = "completion-forge",
    version,
    about = "Generate shell completions from OpenAPI specs"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate shell completion files from an `OpenAPI` spec
    Generate {
        /// Path to the `OpenAPI` YAML or JSON spec file
        #[arg(long, short)]
        spec: PathBuf,

        /// Output directory for generated files
        #[arg(long, short, default_value = ".")]
        output: PathBuf,

        /// CLI command name (defaults to spec info.title, kebab-cased)
        #[arg(long)]
        name: Option<String>,

        /// Prompt icon (Unicode glyph)
        #[arg(long, default_value = "")]
        icon: String,

        /// Command aliases (comma-separated)
        #[arg(long, default_value = "")]
        aliases: String,

        /// Output format: skim-tab, fish, or all
        #[arg(long, short, default_value = "all")]
        format: String,

        /// Grouping strategy: auto, tag, path, or operation-id
        #[arg(long, short, default_value = "auto")]
        grouping: String,
    },

    /// Parse and display grouped operations summary (for debugging)
    Inspect {
        /// Path to the `OpenAPI` spec file
        #[arg(long, short)]
        spec: PathBuf,

        /// Grouping strategy: auto, tag, path, or operation-id
        #[arg(long, short, default_value = "auto")]
        grouping: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Generate {
            spec,
            output,
            name,
            icon,
            aliases,
            format,
            grouping,
        } => {
            let content = std::fs::read_to_string(&spec)
                .with_context(|| format!("failed to read spec: {}", spec.display()))?;

            let openapi: spec::OpenApiSpec = if spec.extension().is_some_and(|e| e == "json") {
                serde_json::from_str(&content)?
            } else {
                serde_yaml_ng::from_str(&content)?
            };

            let cli_name = name.unwrap_or_else(|| {
                use heck::ToKebabCase;
                openapi.info.title.to_kebab_case()
            });

            let alias_list: Vec<String> = aliases
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();

            let strategy = convert::GroupingStrategy::from_str_loose(&grouping);
            let format = r#gen::Format::from_str_loose(&format);

            let completion_spec =
                convert::convert(&openapi, &cli_name, &icon, &alias_list, strategy)
                    .context("failed to convert spec")?;

            let generated = r#gen::generate(&completion_spec, &output, format)
                .context("failed to generate completions")?;

            for path in &generated {
                println!("Generated: {path}");
            }
            println!(
                "\nCompleted: {} ({} groups, {} files)",
                cli_name,
                completion_spec.groups.len(),
                generated.len()
            );
        }

        Command::Inspect { spec, grouping } => {
            let content = std::fs::read_to_string(&spec)
                .with_context(|| format!("failed to read spec: {}", spec.display()))?;

            let openapi: spec::OpenApiSpec = if spec.extension().is_some_and(|e| e == "json") {
                serde_json::from_str(&content)?
            } else {
                serde_yaml_ng::from_str(&content)?
            };

            let strategy = convert::GroupingStrategy::from_str_loose(&grouping);

            let completion_spec =
                convert::convert(&openapi, &openapi.info.title, "", &[], strategy)
                    .context("failed to convert spec")?;

            println!("Name: {}", completion_spec.name);
            println!("Description: {}", completion_spec.description);
            println!("\nGroups ({}):", completion_spec.groups.len());
            for group in &completion_spec.groups {
                println!(
                    "  {} {} — {} ({} ops, {} flags)",
                    group.glyph,
                    group.name,
                    group.description,
                    group.operations.len(),
                    group.flags.len(),
                );
                for op in &group.operations {
                    println!("    {} {} — {}", op.method, op.name, op.description);
                }
            }
        }
    }

    Ok(())
}
