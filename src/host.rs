use std::collections;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::ops::BitAnd;
use std::os::unix::fs::PermissionsExt;
use std::path;
use std::process;
use crate::http::*;

const DOCUMENT_ROOT: &str = "/home/accts/bnc24/cs434/projects/p1/static";
const HTTP_VERSION: &str = "HTTP/1.1";

#[derive(Debug)]
struct Error {
    status: StatusCode,
    message: Option<String>,
}
impl std::error::Error for Error {}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "{}{}{}",
            self.status.to_string(),
            if self.message.is_some() { ": " } else { "" },
            self.message.unwrap_or("".to_string()),
        )?;
        Ok(())
    }
}
impl From<io::Error> for Error {
    fn from(_: io::Error) -> Self {
        Error { status: StatusCode::InternalServerError, message: None }
    }
}

pub struct Host {}
impl Host {
    pub fn new() -> Host {
        Host {}
    }
}
impl RequestHandler for Host {
    fn handle(&self, request: Request) -> Response {
        match request.header.request_line.method {
            Method::Get => handle_get(request),
            Method::Post => handle_post(request),
        }
    }
}

fn parse_path(request_target: &str) -> Result<path::PathBuf, Error> {
    // assume correctly formed - will be provided, not constructed
    let root_path = path::Path::new(DOCUMENT_ROOT).canonicalize().unwrap();

    let mut request_target = path::Path::new(&request_target);
    if request_target.has_root() {
        request_target = request_target.strip_prefix("/").unwrap();
    }
    let path = match path::Path::join(&root_path, request_target).canonicalize() {
        Ok(path) => path,
        Err(_) => return Err(Error { status: StatusCode::NotFound, message: None }),
    };
    println!("-- path: {:#?} --", path);
    if !path.starts_with(root_path) {
        return Err(Error { status: StatusCode::Forbidden, message: None })
    }
    Ok(path)
}

fn handle_get(request: Request) -> Response {
    let path = match parse_path(&request.header.request_line.request_target) {
        Ok(path) => path,
        Err(Error { status, message: _ }) => return error_response(status),
    };

    let metadata = match path.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return error_response(StatusCode::NotFound),
    };

    if metadata.is_dir() {
        // look for index.html under this path
    }

    if metadata.permissions().mode().bitand(0o1).eq(&0o1) {
        return handle_cgi(&path, request);
    }

    match fs::read_to_string(path) {
        Ok(file_content) => {
            Response {
                header: ResponseHeader {
                    status_line: StatusLine {
                        status_code: StatusCode::Ok,
                        http_version: String::from(HTTP_VERSION),
                    },
                    header_lines: vec![("Content-Length".to_string(), (file_content.len() + 2).to_string())],
                },
                body: file_content,
            }
        },
        Err(e) => {
            println!("-- fs error --");
            println!("{}", e);
            let status_code = match e.kind() {
                io::ErrorKind::NotFound => StatusCode::NotFound,
                _ => StatusCode::InternalServerError
            };
            error_response(status_code)
        },
    }
}

fn handle_post(request: Request) -> Response {
    let path = match parse_path(&request.header.request_line.request_target) {
        Ok(path) => path,
        Err(Error { status, message: _ }) => return error_response(status),
    };

    return handle_cgi(&path, request);
}


fn handle_cgi(path: &path::Path, request: Request) -> Response {
    let request_method = request.header.request_line.method.to_string();
    let envs: collections::HashMap<&str, &str> = [
        ("QUERY_STRING", ""),
        ("REMOTE_ADDR", ""),
        ("REMOTE_HOST", ""),
        ("REMOTE_IDENT", ""),
        ("REMOTE_USER", ""),
        ("REQUEST_METHOD", &request_method),
        ("SERVER_NAME", ""),
        ("SERVER_PORT", ""),
        ("SERVER_PROTOCOL", ""),
        ("SERVER_SOFTWARE", ""),
    ].iter()
        .cloned()
        .collect();
    process::Command::new(path)
        .envs(envs)
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .spawn()
        .map_err(|e| e.into())
        .and_then(|mut child| {
            println!("-- body: {} --", request.body);
            child.stdin
                .take()
                .unwrap()
                .write_all(request.body.as_bytes())
                .map(|_| child)
                .map_err(|e| e.into())
        })
        .and_then(|mut child| {
            let mut s = String::new();
            child.stdout
                .take()
                .unwrap()
                .read_to_string(&mut s)
                .map(|_| s)
                .map_err(|e| e.into())
        })
        .and_then(|mut stdout| process_cgi_output(&mut stdout))
        .map(|(mut headers, body)| {
            headers.push(("Content-Length".to_string(), (body.len() + 2).to_string()));
            Response {
                header: ResponseHeader {
                    status_line: StatusLine {
                        status_code: StatusCode::Ok,
                        http_version: String::from(HTTP_VERSION),
                    },
                    header_lines: headers,
                },
                body: body,
            }
        })
        .unwrap_or_else(|_| error_response(StatusCode::InternalServerError))
}

fn process_cgi_output(s: &mut str) -> Result<(Vec<(String, String)>, String), Error> {
    println!("-- cgi output --");
    println!("{}", s);
    let mut sections = s.split("\n\n");
    let headers = sections.next()
        .and_then(|headers| {
            headers.split("\n")
                .fold(Some(Vec::new()), |vec, header| {
                    vec.and_then(|mut vec| {
                        header.split_once(":")
                            .map(|(field, value)| (field.to_string(), value.trim().to_string()))
                            .map(|header| vec.push(header))
                            .map(|_| vec)
                    })
                })
        })
        .ok_or(Error {
            status: StatusCode::InternalServerError,
            message: Some("Could not parse headers from CGI response".to_string())
        })?;
    let body = sections.next().unwrap_or("").to_string();
    // println!("-- cgi parsed --");
    // println!("{:?}", headers);
    // println!("{:?}", body);
    Ok((headers, body))
}

fn error_response(status_code: StatusCode) -> Response {
    Response {
        header: ResponseHeader {
            status_line: StatusLine {
                status_code,
                http_version: String::from(HTTP_VERSION),
            },
            header_lines: Vec::new(),
        },
        body: String::new(),
    }
}
