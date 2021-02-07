use std::error;
use std::{convert, fmt, result, str};

#[allow(unused)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Error {
    /// Generic error type containing a string
    Other(String),
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
    Connector(String),
    /// memflow core error
    Core(memflow::error::Error),
    /// memflow win32 error
    Win32(memflow_win32::error::Error),
    /// memflow partial error
    PartialError(memflow::error::PartialError<()>),
    /// PE error.
    ///
    /// Catch-all for pe related errors.
    PE(pelite::Error),
    /// Tonic transportation error
    TonicTransport(String),
    /// Tonic RPC status error
    TonicStatus(String),
}

/// Convert from &str to error
impl convert::From<&'static str> for Error {
    fn from(error: &'static str) -> Self {
        Error::Other(error.to_string())
    }
}

/// Convert from memflow::error::Error to error
impl convert::From<memflow::error::Error> for Error {
    fn from(error: memflow::error::Error) -> Self {
        Error::Core(error)
    }
}

/// Convert from memflow_win32::error::Error to error
impl convert::From<memflow_win32::error::Error> for Error {
    fn from(error: memflow_win32::error::Error) -> Self {
        Error::Win32(error)
    }
}

impl convert::From<memflow::error::PartialError<()>> for Error {
    fn from(error: memflow::error::PartialError<()>) -> Self {
        Error::PartialError(error)
    }
}

/// Convert from pelite::Error to error
impl From<pelite::Error> for Error {
    fn from(error: pelite::Error) -> Self {
        Error::PE(error)
    }
}

/// Convert from tonic::transport::Error to error
impl From<tonic::transport::Error> for Error {
    fn from(error: tonic::transport::Error) -> Self {
        Error::TonicTransport(format!("{:#?}", error))
    }
}

/// Convert from tonic::transport::Error to error
impl From<tonic::Status> for Error {
    fn from(error: tonic::Status) -> Self {
        Error::TonicTransport(format!("{:#?}", error))
    }
}

impl Error {
    /// Returns a tuple representing the error description and its string value.
    pub fn to_str_pair<'a>(&'a self) -> (&'a str, Option<&'a str>) {
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
            Error::PartialError(e) => ("memflow partial error", Some(e.to_str())),
            Error::PE(e) => ("error handling pe", Some(e.to_str())),
            Error::TonicTransport(e) => ("Tonic transport error", Some(e)),
            Error::TonicStatus(e) => ("Tonic status error", Some(e)),
        }
    }

    /// Returns a simple string representation of the error.
    pub fn to_str(&self) -> &str {
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
