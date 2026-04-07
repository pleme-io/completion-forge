//! Generate shell completions from `OpenAPI` specs.
//!
//! Pipeline: `OpenAPI` 3.0 YAML/JSON → intermediate representation → shell completion files.

pub mod convert;
pub mod r#gen;
pub mod ir;
pub mod spec;
