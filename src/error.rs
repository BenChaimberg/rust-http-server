use std::io;
use std::str;

#[derive(Debug)]
pub struct Error {
    pub message: String,
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
        Error { message: format!("IO error: {:?}", e.kind()) }
    }
}
impl From<str::Utf8Error> for Error {
    fn from(_: str::Utf8Error) -> Self {
        Error { message: "Could not interpret a sequence of u8 as a string".to_string() }
    }
}
