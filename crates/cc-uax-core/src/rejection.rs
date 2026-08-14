use std::fmt;

/// Why a byte buffer cannot be analyzed as a versioned UE5 editor package.
///
/// The distinction is load-bearing for callers that scan many files: an
/// out-of-scope package is truthful `unsupported` evidence about a real asset,
/// while a malformed one is a failure to read something that claimed to be a
/// package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageRejection {
    /// A readable package that this tool deliberately does not target: UE4
    /// packages (`FileVersionUE5` = 0), unversioned or cooked packages,
    /// big-endian console packages, UE3 packages, package-level compression, and
    /// any `FileVersionUE5` below the supported floor.
    OutOfScope,
    /// The bytes do not form a package this tool can read: wrong package magic,
    /// or a declared table count that does not fit the remaining file.
    Malformed,
}

/// Marker error attached to an out-of-scope rejection so [`PackageParseError`]
/// can tell a deliberate scope decision apart from a genuine read failure.
#[derive(Debug)]
struct OutOfScopePackage(String);

impl fmt::Display for OutOfScopePackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OutOfScopePackage {}

/// Builds an error that classifies as [`PackageRejection::OutOfScope`].
pub(crate) fn out_of_scope(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(OutOfScopePackage(message.into()))
}

/// The error returned when a package cannot be parsed at all.
#[derive(Debug)]
pub struct PackageParseError {
    rejection: PackageRejection,
    error: anyhow::Error,
}

impl PackageParseError {
    pub fn rejection(&self) -> PackageRejection {
        self.rejection
    }

    /// True when the file is a readable package that this tool deliberately does
    /// not target. Callers should record it as `unsupported` evidence rather than
    /// as a read/parse failure.
    pub fn is_out_of_scope(&self) -> bool {
        matches!(self.rejection, PackageRejection::OutOfScope)
    }
}

impl From<anyhow::Error> for PackageParseError {
    fn from(error: anyhow::Error) -> Self {
        // Walk the whole chain rather than downcasting the outermost error so a
        // future `.context(..)` on the parse path cannot silently reclassify an
        // out-of-scope package as malformed.
        let rejection = if error.chain().any(|cause| cause.is::<OutOfScopePackage>()) {
            PackageRejection::OutOfScope
        } else {
            PackageRejection::Malformed
        };
        Self { rejection, error }
    }
}

impl fmt::Display for PackageParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.error)
    }
}

impl std::error::Error for PackageParseError {}
