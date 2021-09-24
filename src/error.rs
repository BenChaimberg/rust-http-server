use std::io;
use std::str;
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
        Error::new(format!("IO error: {:?}", e.kind()))
    }
}
impl From<str::Utf8Error> for Error {
    fn from(_: str::Utf8Error) -> Self {
        Error::new("Could not interpret a sequence of u8 as a string".to_string())
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
        HttpError { status: StatusCode::InternalServerError, message: None }
    }
}
