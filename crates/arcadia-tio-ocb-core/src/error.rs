use std::fmt;

/// A crate-local result type for Arcadia OCB reader operations.
pub type Result<T> = std::result::Result<T, ArcadiaTioError>;

/// Stable error codes shared with the broader Arcadia TIO Rust surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ArcadiaTioErrorCode {
    /// Operation completed successfully.
    Ok = 0,
    /// Caller supplied invalid input or violated API preconditions.
    InvalidArgument = 1,
    /// Operation is not implemented.
    Unimplemented = 2,
    /// Low-level I/O failure.
    Io = 3,
    /// Reserved legacy decode/verify failure code.
    Flatbuffers = 4,
}

impl ArcadiaTioErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            ArcadiaTioErrorCode::Ok => "ok",
            ArcadiaTioErrorCode::InvalidArgument => "invalid_argument",
            ArcadiaTioErrorCode::Unimplemented => "unimplemented",
            ArcadiaTioErrorCode::Io => "io",
            ArcadiaTioErrorCode::Flatbuffers => "flatbuffers",
        }
    }
}

/// Structured OCB failure cause metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcbFailureCause {
    /// Caller input or OCB operation preconditions are invalid.
    InvalidInput,
    /// The file is not a supported OCB file or uses an unsupported OCB revision.
    UnsupportedFormat,
    /// The file appears corrupt, torn, truncated, or internally inconsistent.
    CorruptFile,
    /// A cooperating OCB mutation lock is already held or unavailable.
    LockUnavailable,
}

impl OcbFailureCause {
    pub const fn as_str(self) -> &'static str {
        match self {
            OcbFailureCause::InvalidInput => "invalid_input",
            OcbFailureCause::UnsupportedFormat => "unsupported_format",
            OcbFailureCause::CorruptFile => "corrupt_file",
            OcbFailureCause::LockUnavailable => "lock_unavailable",
        }
    }
}

/// Errors surfaced by the source-visible OCB core reader crate.
#[derive(Debug)]
pub enum ArcadiaTioError {
    /// Placeholder for APIs that are not implemented yet.
    Unimplemented(&'static str),
    /// The caller provided invalid input.
    InvalidArgument(&'static str),
    /// Structured OCB failure that preserves ordinary error behavior.
    Ocb {
        code: ArcadiaTioErrorCode,
        cause: OcbFailureCause,
        message: &'static str,
    },
    /// Wrapper for I/O errors.
    Io(std::io::Error),
}

impl ArcadiaTioError {
    /// Return the stable machine-readable error code for this error.
    pub const fn code(&self) -> ArcadiaTioErrorCode {
        match self {
            ArcadiaTioError::Unimplemented(_) => ArcadiaTioErrorCode::Unimplemented,
            ArcadiaTioError::InvalidArgument(_) => ArcadiaTioErrorCode::InvalidArgument,
            ArcadiaTioError::Ocb { code, .. } => *code,
            ArcadiaTioError::Io(_) => ArcadiaTioErrorCode::Io,
        }
    }

    /// Return structured OCB cause metadata when the error came from OCB validation.
    pub const fn ocb_failure_cause(&self) -> Option<OcbFailureCause> {
        match self {
            ArcadiaTioError::Ocb { cause, .. } => Some(*cause),
            _ => None,
        }
    }

    pub const fn ocb_invalid_input(message: &'static str) -> Self {
        ArcadiaTioError::Ocb {
            code: ArcadiaTioErrorCode::InvalidArgument,
            cause: OcbFailureCause::InvalidInput,
            message,
        }
    }

    pub const fn ocb_unsupported_format(message: &'static str) -> Self {
        ArcadiaTioError::Ocb {
            code: ArcadiaTioErrorCode::InvalidArgument,
            cause: OcbFailureCause::UnsupportedFormat,
            message,
        }
    }

    pub const fn ocb_corrupt_file(message: &'static str) -> Self {
        ArcadiaTioError::Ocb {
            code: ArcadiaTioErrorCode::InvalidArgument,
            cause: OcbFailureCause::CorruptFile,
            message,
        }
    }

    pub const fn ocb_lock_unavailable(message: &'static str) -> Self {
        ArcadiaTioError::Ocb {
            code: ArcadiaTioErrorCode::Io,
            cause: OcbFailureCause::LockUnavailable,
            message,
        }
    }
}

impl fmt::Display for ArcadiaTioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArcadiaTioError::Unimplemented(message) => write!(f, "unimplemented: {message}"),
            ArcadiaTioError::InvalidArgument(message) => write!(f, "invalid argument: {message}"),
            ArcadiaTioError::Ocb { code, message, .. } => match code {
                ArcadiaTioErrorCode::Unimplemented => write!(f, "unimplemented: {message}"),
                ArcadiaTioErrorCode::InvalidArgument => write!(f, "invalid argument: {message}"),
                ArcadiaTioErrorCode::Io => write!(f, "io error: {message}"),
                ArcadiaTioErrorCode::Ok | ArcadiaTioErrorCode::Flatbuffers => {
                    write!(f, "{message}")
                }
            },
            ArcadiaTioError::Io(err) => write!(f, "io error: {err}"),
        }
    }
}

impl std::error::Error for ArcadiaTioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ArcadiaTioError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ArcadiaTioError {
    fn from(err: std::io::Error) -> Self {
        ArcadiaTioError::Io(err)
    }
}
