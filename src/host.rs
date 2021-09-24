use std::ops::BitAnd;
use std::os::unix::fs::PermissionsExt;
use std::path;
use crate::config::*;
use crate::cgi;
use crate::error;
use crate::files;
use crate::http::*;

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
        let host_path = match request.header.header_lines.get("Host") {
            Some(p) => p,
            None => return error_response::<String>(StatusCode::BadRequest, None),
        };
        let virtual_host = get_virtual_host(&self.server_config.virtual_hosts, host_path);
        let path = match parse_path(&virtual_host.document_root, &request.header.request_line.request_path) {
            Ok(path) => path,
            Err(error::HttpError { status, message }) => {
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

        self.files.get_content(path)
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

fn parse_path(root_path: &path::PathBuf, request_target: &str) -> Result<path::PathBuf, error::HttpError> {
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
    Ok(path)
}
