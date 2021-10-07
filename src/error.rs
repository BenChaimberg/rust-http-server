use std::io;
use std::net;
use std::num;
use std::str;
use std::sync::mpsc;
use std::time;
use crate::http::StatusCode;

#[derive(Debug)]
pub struct Error {
    pub message: String,
}
impl Error {
    pub fn new(message: String) -> Error {
        Error { message }
    }
}
impl std::error::Error for Error {}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.message)?;
        Ok(())
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::new(e.to_string())
    }
}
impl From<str::Utf8Error> for Error {
    fn from(e: str::Utf8Error) -> Self {
        Error::new(e.to_string())
    }
}
impl From<num::ParseIntError> for Error {
    fn from(e: num::ParseIntError) -> Self {
        Error::new(e.to_string())
    }
}
impl From<time::SystemTimeError> for Error {
    fn from(e: time::SystemTimeError) -> Self {
        Error::new(e.to_string())
    }
}
impl From<mpsc::RecvError> for Error {
    fn from(e: mpsc::RecvError) -> Self {
        Error::new(e.to_string())
    }
}
impl <T> From<mpsc::SendError<T>> for Error {
    fn from(e: mpsc::SendError<T>) -> Self {
        Error::new(e.to_string())
    }
}
impl From<net::AddrParseError> for Error {
    fn from(e: net::AddrParseError) -> Self {
        Error::new(e.to_string())
    }
}
impl From<()> for Error {
    fn from(_: ()) -> Self {
        Error::new("".to_string())
    }
}

#[derive(Debug)]
pub struct HttpError {
    pub status: StatusCode,
    pub message: Option<String>,
}
impl std::error::Error for HttpError {}
impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "{}{}{}",
            self.status.to_string(),
            if self.message.is_some() { ": " } else { "" },
            self.message.as_ref().unwrap_or(&String::new())
        )?;
        Ok(())
    }
}
impl From<io::Error> for HttpError {
    fn from(_: io::Error) -> Self {
        HttpError { status: StatusCode::InternalServerError, message: Some("io::Error".to_string()) }
    }
}
