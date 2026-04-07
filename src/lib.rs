//! Generate shell completions from `OpenAPI` specs.
//!
//! Pipeline: `OpenAPI` 3.0 YAML/JSON → intermediate representation → shell completion files.

/// `OpenAPI` spec → completion IR conversion (grouping strategies, converter trait).
pub mod convert;
/// Typed error types for completion-forge.
pub mod error;
/// Output generators (skim-tab YAML, fish shell).
pub mod r#gen;
/// Intermediate representation types for shell completions.
pub mod ir;
/// `OpenAPI` 3.0 spec types re-exported from sekkei, plus extension traits.
pub mod spec;
