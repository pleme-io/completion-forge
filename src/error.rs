//! Typed error types for completion-forge.

use std::path::PathBuf;

/// Errors that can occur during completion generation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ForgeError {
    /// I/O error with filesystem path context.
    #[error("I/O error at `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// YAML serialization/deserialization failure.
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),

    /// JSON serialization/deserialization failure.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A format-specific generator failed.
    #[error("{format} generation failed")]
    Generate {
        format: &'static str,
        #[source]
        source: Box<ForgeError>,
    },
}

impl ForgeError {
    /// Wrap an `io::Error` with the path that caused it.
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Wrap a generation sub-error with the format name.
    #[must_use]
    pub fn generate(format: &'static str, source: Self) -> Self {
        Self::Generate {
            format,
            source: Box::new(source),
        }
    }
}

/// An unrecognised string was passed to a `FromStr` impl.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unknown variant: `{0}`")]
pub struct ParseEnumError(pub String);

/// Convenience alias used throughout the crate.
pub type ForgeResult<T> = std::result::Result<T, ForgeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forge_error_io_display() {
        let err = ForgeError::io(
            "/some/path",
            std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        );
        let msg = err.to_string();
        assert!(msg.contains("/some/path"), "should contain path: {msg}");
        assert!(msg.contains("not found"), "should contain cause: {msg}");
    }

    #[test]
    fn forge_error_generate_display() {
        let inner = ForgeError::io(
            "/out",
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        );
        let err = ForgeError::generate("fish", inner);
        let msg = err.to_string();
        assert!(msg.contains("fish"), "should mention format: {msg}");
        assert!(
            msg.contains("generation failed"),
            "should say 'generation failed': {msg}"
        );
    }

    #[test]
    fn forge_error_source_chain() {
        use std::error::Error;
        let inner = ForgeError::io(
            "/path",
            std::io::Error::new(std::io::ErrorKind::Other, "boom"),
        );
        let err = ForgeError::generate("skim-tab", inner);
        let source = err.source().expect("should have source");
        assert!(source.to_string().contains("/path"));
    }

    #[test]
    fn parse_enum_error_display() {
        let err = ParseEnumError("badvalue".into());
        assert_eq!(err.to_string(), "unknown variant: `badvalue`");
    }

    #[test]
    fn forge_error_yaml_from() {
        let yaml_err: Result<serde_yaml_ng::Value, _> = serde_yaml_ng::from_str("{{{");
        let err: ForgeError = yaml_err.unwrap_err().into();
        assert!(matches!(err, ForgeError::Yaml(_)));
        assert!(err.to_string().contains("YAML error"));
    }

    #[test]
    fn forge_error_json_from() {
        let json_err: Result<serde_json::Value, _> = serde_json::from_str("{{{");
        let err: ForgeError = json_err.unwrap_err().into();
        assert!(matches!(err, ForgeError::Json(_)));
        assert!(err.to_string().contains("JSON error"));
    }
}
