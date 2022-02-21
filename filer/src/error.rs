use std::num::NonZeroI32;
use std::prelude::v1::*;
use std::{fmt, result, str};

use cglue::result::IntError;

#[cfg(feature = "std")]
use std::error;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Error(pub ErrorOrigin, pub ErrorKind);

impl Error {
    /// Returns a static string representing the type of error.
    pub fn as_str(&self) -> &'static str {
        self.1.to_str()
    }

    /// Returns a static string representing the type of error.
    pub fn into_str(self) -> &'static str {
        self.as_str()
    }
}

impl IntError for Error {
    fn into_int_err(self) -> NonZeroI32 {
        let origin = ((self.0 as i32 + 1) & 0xFFFi32) << 4;
        let kind = ((self.1 as i32 + 1) & 0xFFFi32) << 16;
        NonZeroI32::new(-(1 + origin + kind)).unwrap()
    }

    fn from_int_err(err: NonZeroI32) -> Self {
        let origin = ((-err.get() - 1) >> 4i32) & 0xFFFi32;
        let kind = ((-err.get() - 1) >> 16i32) & 0xFFFi32;

        let error_origin = if origin > 0 && origin <= ErrorOrigin::Other as i32 + 1 {
            unsafe { std::mem::transmute(origin as u16 - 1) }
        } else {
            ErrorOrigin::Other
        };

        let error_kind = if kind > 0 && kind <= ErrorKind::Unknown as i32 + 1 {
            unsafe { std::mem::transmute(kind as u16 - 1) }
        } else {
            ErrorKind::Unknown
        };

        Self(error_origin, error_kind)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.0.to_str(), self.1.to_str())
    }
}

#[cfg(feature = "std")]
impl error::Error for Error {
    fn description(&self) -> &str {
        self.as_str()
    }
}

impl From<ErrorOrigin> for Error {
    fn from(origin: ErrorOrigin) -> Self {
        Error(origin, ErrorKind::Unknown)
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Error(ErrorOrigin::Other, kind)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error(ErrorOrigin::Io, ErrorKind::Unknown)
    }
}

#[repr(u16)]
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ErrorOrigin {
    Backend,
    Node,
    Leaf,
    Branch,
    Read,
    Write,
    Rpc,
    Io,

    Other,
}

impl ErrorOrigin {
    /// Returns a static string representing the type of error.
    pub fn to_str(self) -> &'static str {
        match self {
            ErrorOrigin::Backend => "backend",
            ErrorOrigin::Node => "node",
            ErrorOrigin::Leaf => "leaf",
            ErrorOrigin::Branch => "branch",
            ErrorOrigin::Read => "read",
            ErrorOrigin::Write => "write",
            ErrorOrigin::Rpc => "rpc",
            ErrorOrigin::Io => "io",
            ErrorOrigin::Other => "other",
        }
    }
}

#[repr(u16)]
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ErrorKind {
    Uninitialized,
    NotSupported,
    NotImplemented,
    Configuration,
    Offset,

    InvalidArgument,

    NotFound,
    OutOfBounds,

    InvalidPath,
    ReadOnly,
    UnableToReadDir,
    UnableToReadDirEntry,
    UnableToReadFile,
    UnableToCreateDirectory,
    UnableToWriteFile,
    UnableToSeekFile,

    VersionMismatch,
    AlreadyExists,
    PluginNotFound,
    InvalidAbi,

    Unknown,
}

impl ErrorKind {
    /// Returns a static string representing the type of error.
    pub fn to_str(self) -> &'static str {
        match self {
            ErrorKind::Uninitialized => "unitialized",
            ErrorKind::NotSupported => "not supported",
            ErrorKind::NotImplemented => "not implemented",
            ErrorKind::Configuration => "configuration error",
            ErrorKind::Offset => "offset error",

            ErrorKind::InvalidArgument => "invalid argument passed",

            ErrorKind::NotFound => "not found",
            ErrorKind::OutOfBounds => "out of bounds",

            ErrorKind::InvalidPath => "invalid path",
            ErrorKind::ReadOnly => "trying to write to a read only resource",
            ErrorKind::UnableToReadDir => "unable to read directory",
            ErrorKind::UnableToReadDirEntry => "unable to read directory entry",
            ErrorKind::UnableToReadFile => "unable to read file",
            ErrorKind::UnableToCreateDirectory => "unable to create directory",
            ErrorKind::UnableToWriteFile => "unable to write file",
            ErrorKind::UnableToSeekFile => "unable to seek file",

            ErrorKind::VersionMismatch => "version mismatch",
            ErrorKind::AlreadyExists => "already exists",
            ErrorKind::PluginNotFound => "plugin not found",
            ErrorKind::InvalidAbi => "invalid plugin ABI",

            ErrorKind::Unknown => "unknown error",
        }
    }
}

/// Specialized `Result` type for memflow results.
pub type Result<T> = result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use cglue::result::{
        from_int_result, from_int_result_empty, into_int_out_result, into_int_result, IntError,
    };
    use std::mem::MaybeUninit;
    use std::num::NonZeroI32;

    #[test]
    pub fn error_from_i32_invalid() {
        let mut err = Error::from_int_err(NonZeroI32::new(std::i32::MIN + 1).unwrap());
        assert_eq!(err.0, ErrorOrigin::Other);
        assert_eq!(err.1, ErrorKind::Unknown);

        err = Error::from_int_err(NonZeroI32::new(-1).unwrap());
        assert_eq!(err.0, ErrorOrigin::Other);
        assert_eq!(err.1, ErrorKind::Unknown);

        err = Error::from_int_err(NonZeroI32::new(-2).unwrap());
        assert_eq!(err.0, ErrorOrigin::Other);
        assert_eq!(err.1, ErrorKind::Unknown);

        err = Error::from_int_err(NonZeroI32::new(-3).unwrap());
        assert_eq!(err.0, ErrorOrigin::Other);
        assert_eq!(err.1, ErrorKind::Unknown);
    }

    #[test]
    pub fn error_to_from_i32() {
        let err = Error::from_int_err(
            Error(ErrorOrigin::Other, ErrorKind::InvalidArgument).into_int_err(),
        );
        assert_eq!(err.0, ErrorOrigin::Other);
        assert_eq!(err.1, ErrorKind::InvalidArgument);
    }

    #[test]
    pub fn result_ok_void_ffi() {
        let r: Result<()> = Ok(());
        let result: Result<()> = from_int_result_empty(into_int_result(r));
        assert!(result.is_ok());
    }

    #[test]
    pub fn result_ok_value_ffi() {
        let r: Result<i32> = Ok(1234i32);
        let mut out = MaybeUninit::<i32>::uninit();
        let result: Result<i32> = unsafe { from_int_result(into_int_out_result(r, &mut out), out) };
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1234i32);
    }

    #[test]
    pub fn result_error_void_ffi() {
        let r: Result<i32> = Err(Error(ErrorOrigin::Other, ErrorKind::InvalidArgument));
        let result: Result<()> = from_int_result_empty(into_int_result(r));
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, ErrorOrigin::Other);
        assert_eq!(result.err().unwrap().1, ErrorKind::InvalidArgument);
    }

    #[test]
    pub fn result_error_value_ffi() {
        let r: Result<i32> = Err(Error(ErrorOrigin::Other, ErrorKind::InvalidArgument));
        let mut out = MaybeUninit::<i32>::uninit();
        let result: Result<i32> = unsafe { from_int_result(into_int_out_result(r, &mut out), out) };
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, ErrorOrigin::Other);
        assert_eq!(result.err().unwrap().1, ErrorKind::InvalidArgument);
    }
}
