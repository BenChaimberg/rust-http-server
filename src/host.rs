use std::convert::TryInto;
use std::ops::BitAnd;
use std::os::unix::fs::PermissionsExt;
use std::path;
use std::time;
use crate::config::*;
use crate::cgi;
use crate::error;
use crate::files;
use crate::http::*;
use crate::time::parse_date_1123;

pub struct Host<'a> {
    server_config: &'a ServerConfig,
    cgi: cgi::Cgi<'a>,
    files: files::Files,
}

impl<'a> Host<'a> {
    pub fn new(server_config: &'a ServerConfig) -> Host {
        Host { server_config, cgi: cgi::Cgi::new(server_config), files: files::Files::new(server_config.cache_size.unwrap_or(1024)) }
    }
}

impl RequestHandler for Host<'_> {
    fn handle(&self, request: Request) -> Response {
        let host_path = match request.header.header_lines.get(&HeaderField::Host) {
            Some(p) => p,
            None => return error_response::<String>(StatusCode::BadRequest, None),
        };
        let virtual_host = get_virtual_host(&self.server_config.virtual_hosts, host_path);
        let request_target = match parse_path(&virtual_host.document_root, &request.header.request_line.request_path) {
            Ok(path) => path,
            Err(error::HttpError { status, message }) => {
                println!("-- bad parse_path --");
                return error_response(status, message)
            },
        };

        match request.header.request_line.method {
            Method::Get => self.handle_get(request_target, request, virtual_host),
            Method::Post => self.handle_post(request_target, request, virtual_host),
        }.unwrap_or_else(|e| error_response(e.status, e.message))
    }
}

impl<'a> Host<'a> {
    fn handle_get(&self, request_target: RequestTarget, request: Request, virtual_host: &VirtualHost) -> Result<Response, error::HttpError> {
        let (path, metadata) = content_negotiation(request_target, &request.header.header_lines)?;

        if metadata.is_dir() {
            return Err(error::HttpError { status: StatusCode::NotFound, message: None });
        }

        if metadata.permissions().mode().bitand(0o1).eq(&0o1) {
            return self.cgi.handle(path, request, virtual_host);
        }

        if let Some(since) = request.header.header_lines.get(&HeaderField::IfModifiedSince) {
            let since = parse_date_1123(since).map_err(|e| error::HttpError { status: StatusCode::BadRequest, message: Some(e.message) })?;
            let mod_since = files::Files::modified_since(&path, time::Duration::from_secs(since.timestamp().try_into().unwrap())).unwrap_or(true);
            if !mod_since {
                return Ok(
                    Response {
                        header: ResponseHeader {
                            status_line: StatusLine {
                                status_code: StatusCode::NotModified,
                                http_version: String::from(HTTP_VERSION),
                            },
                            header_lines: Vec::new(),
                        },
                        body: String::new(),
                    }
                );
            }
        }

        Ok(self.files.get_content(path))
    }

    fn handle_post(&self, request_target: RequestTarget, request: Request, virtual_host: &VirtualHost) -> Result<Response, error::HttpError> {
        // assert executable
        // assert not directory
        return self.cgi.handle(request_target.path, request, virtual_host);
    }
}

fn metadata_or_400(path: &path::PathBuf) -> Result<std::fs::Metadata, error::HttpError> {
    path.metadata().map_err(|_| error::HttpError { status: StatusCode::NotFound, message: None })
}

fn content_negotiation(request_target: RequestTarget, header_lines: &std::collections::HashMap<HeaderField, String>) -> Result<(path::PathBuf, std::fs::Metadata), error::HttpError> {
    let path = request_target.path;
    if request_target.is_dir {
        if let Some(user_agent) = header_lines.get(&HeaderField::UserAgent) {
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

fn get_virtual_host<'a>(virtual_hosts: &'a Vec<VirtualHost>, host: &str) -> &'a VirtualHost {
    for virtual_host in virtual_hosts.iter() {
        if virtual_host.server_name == host {
            return &virtual_host;
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
