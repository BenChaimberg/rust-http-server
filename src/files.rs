use std::cell::RefCell;
use std::collections;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs;
use std::io;
use std::path;
use std::time;
use crate::error;
use crate::http::*;
use crate::time::to_1123;

const BYTES_PER_KILOBYTE: u32 = 1024;

pub struct Files {
    cache: RefCell<collections::HashMap<path::PathBuf, File>>,
}

struct File {
    content: String,
    modified: time::SystemTime,
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
        let cached = {
            self.cache.borrow().get(&path)
                .map(|File { content, modified }| {
                    // println!("-- cache hit --");
                    File { content: content.to_string(), modified: modified.clone() }
                })
                .ok_or_else(|| {
                    let s = "cache miss";
                    // println!("-- {} --", s);
                    s.to_string();
                })
        };
        let File { content, modified } = cached
            .or_else(|_| {
                fs::read_to_string(&path)
                    .map(|content| {
                        let modified = path.metadata().unwrap().modified().unwrap();
                        self.cache.borrow_mut().insert(path.clone(), File { content: content.clone(), modified, });
                        // println!("-- cache insert --");
                        File { content, modified }
                    })
                    .map_err(|e| {
                        let status = match e.kind() {
                            io::ErrorKind::NotFound => StatusCode::NotFound,
                            _ => StatusCode::InternalServerError
                        };
                        error::HttpError { status, message: Some(e.to_string())}
                    })
            })?;

        let header_lines = {
            let modified_str = to_1123(
                chrono::DateTime::from_utc(
                    chrono::naive::NaiveDateTime::from_timestamp(
                        modified.duration_since(time::UNIX_EPOCH).unwrap().as_secs().try_into().unwrap(),
                        0
                    ),
                    chrono::offset::Utc
                )
            );
            let mut header_lines = HashMap::new();
            header_lines.insert(ResponseHeaderField::ContentLength, (content.len() + 2).to_string());
            header_lines.insert(ResponseHeaderField::LastModified, modified_str);
            if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                let content_type = match extension {
                    "txt" => Some("text/plain"),
                    "html" => Some("text/html"),
                    "jpg" => Some("image/jpeg"),
                    _ => None,
                };
                if let Some(content_type) = content_type {
                    header_lines.insert(ResponseHeaderField::ContentType, content_type.to_string());
                }
            }
            header_lines
        };
        Ok(Response {
            header: ResponseHeader {
                status_line: StatusLine {
                    status_code: StatusCode::Ok,
                    http_version: String::from(HTTP_VERSION),
                },
                header_lines,
            },
            body: content,
        })
    }

    pub fn modified_since(path: &path::PathBuf, start: time::Duration) -> Result<bool, error::Error> {
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;
        let duration = modified.duration_since(time::UNIX_EPOCH)?;
        Ok((duration - start).as_secs() > 0)
    }
}
