use std::cell::RefCell;
use std::collections;
use std::convert::TryInto;
use std::fs;
use std::io;
use std::path;
use std::time;
use crate::error;
use crate::http::*;

const BYTES_PER_KILOBYTE: u32 = 1024;

pub struct Files {
    cache: RefCell<collections::HashMap<path::PathBuf, String>>,
}

impl Files {
    pub fn new(cache_size: u32) -> Files {
        Files {
            cache: RefCell::new(
                collections::HashMap::with_capacity(
                    (cache_size * BYTES_PER_KILOBYTE)
                        .try_into()
                        .unwrap_or(usize::MAX)
                )
            )
        }
    }

    pub fn get_content(&self, path: path::PathBuf) -> Result<Response, error::HttpError> {
        let cached_content = {
            self.cache.borrow().get(&path)
                .map(|content| {
                    // println!("-- cache hit --");
                    content.to_string()
                })
                .ok_or_else(|| {
                    let s = "cache miss";
                    // println!("-- {} --", s);
                    s.to_string();
                })
        };
        cached_content
            .or_else(|_| match fs::read_to_string(&path) {
                Ok(content) => {
                    self.cache.borrow_mut().insert(path.clone(), content.clone());
                    // println!("-- cache insert --");
                    Ok(content)
                },
                Err(e) => {
                    let status = match e.kind() {
                        io::ErrorKind::NotFound => StatusCode::NotFound,
                        _ => StatusCode::InternalServerError
                    };
                    Err(error::HttpError { status, message: Some(e.to_string())})
                },
            })
            .map(|content| {
                let mut header_lines = vec![("Content-Length".to_string(), (content.len() + 2).to_string())];
                if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                    let content_type = match extension {
                        "txt" => Some("text/plain"),
                        "html" => Some("text/html"),
                        "jpg" => Some("image/jpeg"),
                        _ => None,
                    };
                    if let Some(content_type) = content_type {
                        header_lines.push(("Content-Type".to_string(), content_type.to_string()));
                    }
                }
                Response {
                    header: ResponseHeader {
                        status_line: StatusLine {
                            status_code: StatusCode::Ok,
                            http_version: String::from(HTTP_VERSION),
                        },
                        header_lines,
                    },
                    body: content,
                }
            })
    }

    pub fn modified_since(path: &path::PathBuf, start: time::Duration) -> Result<bool, error::Error> {
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;
        let duration = modified.duration_since(time::UNIX_EPOCH)?;
        Ok(duration > start)
    }
}
