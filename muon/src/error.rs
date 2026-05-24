use std::error::Error;
use std::fmt::Display;

use crate::Path;

/// Error types for mutation operations.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MutationError {
    /// The specified path does not exist.
    IndexError {
        /// The path that could not be found.
        path: Path<false>,
    },
    /// Mutation could not be performed at the specified path.
    OperationError {
        /// The path where the operation could not be performed.
        path: Path<false>,
    },
    /// Error applying a truncate operation.
    #[cfg(feature = "truncate")]
    TruncateError {
        /// The path where the truncation failed.
        path: Path<false>,
        /// The actual length of the value being truncated.
        actual_len: usize,
        /// The requested truncation length.
        truncate_len: usize,
    },
}

impl Display for MutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IndexError { path } => {
                write!(f, "path {path} does not exist or is malformed")
            }
            Self::OperationError { path } => {
                write!(f, "operation could not be performed at {path}")
            }
            #[cfg(feature = "truncate")]
            Self::TruncateError {
                path,
                actual_len,
                truncate_len,
            } => {
                write!(
                    f,
                    "cannot truncate at {path}: actual length {actual_len} is less than truncate length {truncate_len}"
                )
            }
        }
    }
}

impl Error for MutationError {}
