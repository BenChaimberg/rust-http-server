use std::collections::HashMap;
use std::convert::TryInto;
use std::ops::BitAnd;
use std::os::unix::fs::PermissionsExt;
use std::path;
use std::str::FromStr;
use std::time;
use crate::config::*;
use crate::cgi;
use crate::error;
use crate::files;
use crate::http::*;
use crate::time::{now_1123, parse_date_1123};

pub struct Host {
    server_config: ServerConfig,
    cgi: cgi::Cgi,
    files: files::Files,
}

impl Host {
    pub fn new(server_config: ServerConfig) -> Host {
        let files = files::Files::new(
            server_config.directives.get(&Directive::CacheSize)
                .and_then(|cache_size| u32::from_str(cache_size).ok())
                .unwrap_or(1024)
        );
        let cgi = cgi::Cgi::new(server_config.clone());
        Host {
            server_config,
            cgi,
            files,
        }
    }

    pub fn handle(&self, request: &Request, overloaded: bool) -> Response {
        let mut response = self.handle_result(request, overloaded).unwrap_or_else(|e| error_response(e.status, e.message));
        response.header.header_lines.insert(ResponseHeaderField::Server, "Rust/0.1".to_string());
        response.header.header_lines.insert(ResponseHeaderField::Date, now_1123());
        response
    }
}

impl Host {
    fn handle_result(&self, request: &Request, overloaded: bool) -> Result<Response, error::HttpError> {
        let host_path = request.header.header_lines.get(&RequestHeaderField::Host)
            .ok_or(error::HttpError { status: StatusCode::BadRequest, message: None })?;
        let virtual_host = get_virtual_host(&self.server_config.virtual_hosts, host_path);

        if request.header.request_line.method == Method::Get && request.header.request_line.request_path == "/load" {
            return heartbeat(overloaded);
        }

        let document_root = &virtual_host.directives.get(&Directive::DocumentRoot)
            .and_then(|document_root| path::Path::new(document_root).canonicalize().ok())
            .ok_or(error::HttpError { status: StatusCode::InternalServerError, message: Some("Could not determine document root for virtual host".to_string()) })?;
        let request_target = parse_path(document_root, &request.header.request_line.request_path)?;

        match request.header.request_line.method {
            Method::Get => self.handle_get(request_target, request, virtual_host),
            Method::Post => self.handle_post(request_target, request, virtual_host),
        }
    }

    fn handle_get(&self, request_target: RequestTarget, request: &Request, virtual_host: &VirtualHost) -> Result<Response, error::HttpError> {
        let (path, metadata) = content_negotiation(request_target, &request.header.header_lines)?;

        if metadata.is_dir() {
            return Err(error::HttpError { status: StatusCode::NotFound, message: None });
        }

        if metadata.permissions().mode().bitand(0o1).eq(&0o1) {
            return self.cgi.handle(path, request, virtual_host);
        }

        if let Some(since) = request.header.header_lines.get(&RequestHeaderField::IfModifiedSince) {
            let since = parse_date_1123(since).map_err(|e| error::HttpError { status: StatusCode::BadRequest, message: Some(e.message) })?;
            let mod_since = files::Files::modified_since(&path, time::Duration::from_secs(since.timestamp().try_into().unwrap())).unwrap_or(true);
            if !mod_since {
                let mut header_lines = HashMap::new();
                header_lines.insert(ResponseHeaderField::ContentLength, "0".to_string());
                return Ok(
                    Response {
                        header: ResponseHeader {
                            status_line: StatusLine {
                                status_code: StatusCode::NotModified,
                                http_version: String::from(HTTP_VERSION),
                            },
                            header_lines,
                        },
                        body: String::new(),
                    }
                );
            }
        }

        self.files.get_content(path)
    }

    fn handle_post(&self, request_target: RequestTarget, request: &Request, virtual_host: &VirtualHost) -> Result<Response, error::HttpError> {
        // assert executable
        // assert not directory
        return self.cgi.handle(request_target.path, request, virtual_host);
    }
}

fn heartbeat(overloaded: bool) -> Result<Response, error::HttpError> {
    let mut header_lines = HashMap::new();
    header_lines.insert(ResponseHeaderField::ContentLength, "0".to_string());
    let status_code = if overloaded { StatusCode::ServiceUnavailable } else { StatusCode::Ok };
    Ok(
        Response {
            header: ResponseHeader {
                status_line: StatusLine {
                    status_code,
                    http_version: String::from(HTTP_VERSION),
                },
                header_lines,
            },
            body: String::new(),
        }
    )
}

fn content_negotiation(request_target: RequestTarget, header_lines: &std::collections::HashMap<RequestHeaderField, String>) -> Result<(path::PathBuf, std::fs::Metadata), error::HttpError> {
    let path = request_target.path;
    if request_target.is_dir {
        if let Some(user_agent) = header_lines.get(&RequestHeaderField::UserAgent) {
            if user_agent.contains("iPhone") || user_agent.contains("Mobile") {
                let mobile_path = path.join("index_m.html");
                let metadata = metadata_or_400(&mobile_path);
                if let Ok(metadata) = metadata {
                    return Ok((mobile_path, metadata));
                }
            }
        }
        let index_path = path.join("index.html");
        let metadata = metadata_or_400(&index_path)?;
        return Ok((index_path, metadata));
    }
    let metadata = metadata_or_400(&path)?;
    Ok((path, metadata))
}

fn metadata_or_400(path: &path::PathBuf) -> Result<std::fs::Metadata, error::HttpError> {
    path.metadata().map_err(|_| error::HttpError { status: StatusCode::NotFound, message: None })
}

fn get_virtual_host<'a>(virtual_hosts: &'a Vec<VirtualHost>, host: &str) -> &'a VirtualHost {
    for virtual_host in virtual_hosts.iter() {
        if let Some(server_name) = virtual_host.directives.get(&Directive::ServerName) {
            if server_name == host {
                return &virtual_host;
            }
        }
    }
    return &virtual_hosts.first().unwrap();
}

fn parse_path(root_path: &path::PathBuf, request_target: &str) -> Result<RequestTarget, error::HttpError> {
    let is_dir = request_target.ends_with("/");
    let mut request_target = path::Path::new(&request_target);
    if request_target.has_root() {
        request_target = request_target.strip_prefix("/").unwrap();
    }
    let path = match path::Path::join(&root_path, request_target).canonicalize() {
        Ok(path) => path,
        Err(_) => return Err(error::HttpError { status: StatusCode::NotFound, message: None }),
    };
    // println!("-- path: {:#?} --", path);
    if !path.starts_with(root_path) {
        return Err(error::HttpError { status: StatusCode::Forbidden, message: None })
    }
    Ok(RequestTarget { path, is_dir })
}

struct RequestTarget {
    path: path::PathBuf,
    is_dir: bool,
}
