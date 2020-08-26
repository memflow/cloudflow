use std::error;
use std::{convert, fmt, result, str};

#[allow(unused)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Error {
    /// Generic error type containing a string
    Other(&'static str),
    /// IO Error
    ///
    /// Catch-all for io errors.
    IO,
    /// Serialization error
    Serialize,
    /// Deserialization error
    Deserialize,
    /// Socket read fail
    ///
    /// Catch-all for socket read errors.
    SocketRead,
    /// Socket write fail
    ///
    /// Catch-all for socket write errors.
    SocketWrite,
    /// GDB stub error
    ///
    /// Catch-all for gdb stub errors
    GDB,
    /// Connector error
    Connector(&'static str),
    /// memflow core error
    Core(memflow_core::error::Error),
    /// memflow win32 error
    Win32(memflow_win32::error::Error),
}

/// Convert from &str to error
impl convert::From<&'static str> for Error {
    fn from(error: &'static str) -> Self {
        Error::Other(error)
    }
}

/// Convert from memflow_core::error::Error to error
impl convert::From<memflow_core::error::Error> for Error {
    fn from(error: memflow_core::error::Error) -> Self {
        Error::Core(error)
    }
}

/// Convert from memflow_win32::error::Error to error
impl convert::From<memflow_win32::error::Error> for Error {
    fn from(error: memflow_win32::error::Error) -> Self {
        Error::Win32(error)
    }
}

impl Error {
    /// Returns a tuple representing the error description and its string value.
    pub fn to_str_pair(self) -> (&'static str, Option<&'static str>) {
        match self {
            Error::Other(e) => ("other error", Some(e)),
            Error::IO => ("i/o error", None),
            Error::Serialize => ("serialization error", None),
            Error::Deserialize => ("deserialization error", None),
            Error::SocketRead => ("socket read error", None),
            Error::SocketWrite => ("socket write error", None),
            Error::GDB => ("gdb stub error", None),
            Error::Connector(e) => ("connector error", Some(e)),
            Error::Core(e) => ("memflow core error", Some(e.to_str())),
            Error::Win32(e) => ("memflow win32 error", Some(e.to_str())),
        }
    }

    /// Returns a simple string representation of the error.
    pub fn to_str(self) -> &'static str {
        self.to_str_pair().0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (desc, value) = self.to_str_pair();

        if let Some(value) = value {
            write!(f, "{}: {}", desc, value)
        } else {
            f.write_str(desc)
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        self.to_str()
    }
}

/// Specialized `Result` type for flow-win32 errors.
pub type Result<T> = result::Result<T, Error>;
