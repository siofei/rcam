//! Fail-closed error taxonomy for `.rcam` encode/decode (§7/§50 of the S4-B2
//! brief). Every reader failure is one of these — never a silent partial
//! load or a dropped object.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// Not a ZIP, or the ZIP structure itself is malformed/truncated.
    MalformedArchive(String),
    /// `../`, an absolute path, or a path outside the archive root.
    PathTraversal(String),
    /// Two entries normalize to the same path.
    DuplicatePath(String),
    /// Entry count, uncompressed size, nesting depth, or string length
    /// exceeds a bounded reader policy limit.
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    /// `manifest.json` is missing.
    ManifestMissing,
    /// An entry's actual SHA-256 does not match its manifest entry.
    HashMismatch { path: String },
    /// `manifest.json`/`project.json` did not parse as valid JSON for its
    /// expected shape.
    SchemaInvalid(String),
    /// A JSON number was `NaN`/`Infinity`/`-Infinity` (§48: finite only).
    NonFiniteValue(String),
    /// `format` field is not `"rcam"`, or `format_version` is not one this
    /// reader knows how to decode (§50: fail-closed, never guess).
    UnknownFormatVersion(u32),
    /// A recognized-but-not-yet-supported feature (e.g. a persisted Drill
    /// layer, §43) was present in an otherwise well-formed file.
    UnsupportedFeature(String),
    /// Decoded successfully but failed `SemanticDocument::validate()` or an
    /// equivalent model-level semantic check.
    SemanticInvalid(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedArchive(message) => write!(f, "malformed .rcam archive: {message}"),
            Self::PathTraversal(path) => write!(f, "unsafe entry path: {path}"),
            Self::DuplicatePath(path) => write!(f, "duplicate entry path: {path}"),
            Self::ResourceLimit {
                resource,
                limit,
                actual,
            } => write!(f, "resource limit {resource}: {actual} > {limit}"),
            Self::ManifestMissing => write!(f, "manifest.json is missing"),
            Self::HashMismatch { path } => write!(f, "sha256 mismatch for {path}"),
            Self::SchemaInvalid(message) => write!(f, "invalid .rcam schema: {message}"),
            Self::NonFiniteValue(field) => write!(f, "non-finite value at {field}"),
            Self::UnknownFormatVersion(version) => {
                write!(f, "unknown .rcam format_version {version}")
            }
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {feature}"),
            Self::SemanticInvalid(message) => write!(f, "invalid project semantics: {message}"),
        }
    }
}

impl std::error::Error for ProjectError {}
