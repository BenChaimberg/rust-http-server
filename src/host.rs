use std::fs;
use std::io;
use std::ops::BitAnd;
use std::os::unix::fs::PermissionsExt;
use std::path;
use crate::config::*;
use crate::cgi;
use crate::http::*;

#[derive(Debug)]
struct HttpError {
    status: StatusCode,
    message: Option<String>,
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

pub struct Host<'a> {
    server_config: &'a ServerConfig,
    cgi: cgi::Cgi<'a>,
}
impl<'a> Host<'a> {
    pub fn new(server_config: &'a ServerConfig) -> Host {
        Host { server_config, cgi: cgi::Cgi::new(server_config) }
    }
}
impl RequestHandler for Host<'_> {
    fn handle(&self, request: Request) -> Response {
        let host_path = match request.header.header_lines.get("Host") {
            Some(p) => p,
            None => return error_response::<String>(StatusCode::BadRequest, None),
        };
        let virtual_host = get_virtual_host(&self.server_config.virtual_hosts, host_path);
        let path = match parse_path(&virtual_host.document_root, &request.header.request_line.request_path) {
            Ok(path) => path,
            Err(HttpError { status, message }) => {
                println!("-- bad parse_path --");
                return error_response(status, message)
            },
        };

        match request.header.request_line.method {
            Method::Get => self.handle_get(path, request, virtual_host),
            Method::Post => self.handle_post(path, request, virtual_host),
        }
    }
}
impl<'a> Host<'a> {
    fn handle_get(&self, path: path::PathBuf, request: Request, virtual_host: &VirtualHost) -> Response {
        let metadata = match path.metadata() {
            Ok(metadata) => metadata,
            Err(_) => return error_response::<String>(StatusCode::NotFound, None),
        };

        if metadata.is_dir() {
            // look for index.html under this path
        }

        if metadata.permissions().mode().bitand(0o1).eq(&0o1) {
            println!("-- permissions {} --", metadata.permissions().mode());
            return self.cgi.handle(path, request, virtual_host);
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
                error_response(status_code, Some(e))
            },
        }
    }

    fn handle_post(&self, path: path::PathBuf, request: Request, virtual_host: &VirtualHost) -> Response {
        // assert executable
        return self.cgi.handle(path, request, virtual_host);
    }
}

fn get_virtual_host<'a>(virtual_hosts: &'a Vec<VirtualHost>, host: &str) -> &'a VirtualHost {
    for virtual_host in virtual_hosts.iter() {
        if virtual_host.server_name == host {
            return &virtual_host;
        }
    }
    return &virtual_hosts.first().unwrap();
}

fn parse_path(root_path: &path::PathBuf, request_target: &str) -> Result<path::PathBuf, HttpError> {
    let mut request_target = path::Path::new(&request_target);
    if request_target.has_root() {
        request_target = request_target.strip_prefix("/").unwrap();
    }
    let path = match path::Path::join(&root_path, request_target).canonicalize() {
        Ok(path) => path,
        Err(_) => return Err(HttpError { status: StatusCode::NotFound, message: None }),
    };
    println!("-- path: {:#?} --", path);
    if !path.starts_with(root_path) {
        return Err(HttpError { status: StatusCode::Forbidden, message: None })
    }
    Ok(path)
}
