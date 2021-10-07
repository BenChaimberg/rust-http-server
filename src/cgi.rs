use std::collections::HashMap;
use std::io::{Read, Write};
use std::path;
use std::process;
use std::str::FromStr;
use crate::config::*;
use crate::error::{Error,HttpError};
use crate::http::*;

pub struct Cgi {
    server_config: ServerConfig,
}
impl Cgi {
    pub fn new(server_config: ServerConfig) -> Cgi {
        Cgi { server_config }
    }
}
impl Cgi {
    pub fn handle(&self, path: path::PathBuf, request: &Request, virtual_host: &VirtualHost) -> Result<Response, HttpError> {
        let remote_addr = request.remote.addr.to_string();
        let request_method = request.header.request_line.method.to_string();
        let internal_error = || -> HttpError { HttpError { status: StatusCode::InternalServerError, message: None } };
        let server_port = self.server_config.directives.get(&Directive::ListenPort).ok_or_else(internal_error)?.to_string();
        let server_name = virtual_host.directives.get(&Directive::ServerName).ok_or_else(internal_error)?;
        let envs: HashMap<&str, &str> = [
            ("QUERY_STRING", request.header.request_line.query_string.as_str()),
            ("REMOTE_ADDR", &remote_addr),
            // ("REMOTE_HOST", ""), NULL if not provided
            // ("REMOTE_IDENT", ""), MAY
            // ("REMOTE_USER", ""), MUST if AUTH_TYPE is Basic or Digest
            ("REQUEST_METHOD", &request_method),
            ("SERVER_NAME", &server_name),
            ("SERVER_PORT", &server_port),
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
                // println!("-- body: {} --", request.body);
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
                headers.insert(ResponseHeaderField::ContentLength, (body.len() + 2).to_string());
                Response {
                    header: ResponseHeader {
                        status_line: StatusLine {
                            status_code: StatusCode::Ok,
                            http_version: String::from(HTTP_VERSION),
                        },
                        header_lines: headers,
                    },
                    body,
                }
            })
            .map_err(|e| {
                println!("-- bad cgi --");
                HttpError { status: StatusCode::InternalServerError, message: Some(e.to_string()) }
            })
    }
}

fn process_cgi_output(s: &mut str) -> Result<(HashMap<ResponseHeaderField, String>, String), Error> {
    // println!("-- cgi output --");
    // println!("{}", s);
    let mut sections = s.split("\n\n");
    let headers = sections.next()
        .and_then(|headers| {
            headers.split("\n")
                .fold(Some(HashMap::new()), |map, header| {
                    map.and_then(|mut map| {
                        header.split_once(":")
                            .and_then(|(field, value)| {
                                ResponseHeaderField::from_str(field).map(|field| (field, value.trim().to_string())).ok()
                            })
                            .map(|(field, value)| map.insert(field, value))
                            .map(|_| map)
                    })
                })
        })
        .ok_or(Error::new("Could not parse headers from CGI response".to_string()))?;
    let body = sections.next().unwrap_or("").to_string();
    // println!("-- cgi parsed --");
    // println!("{:?}", headers);
    // println!("{:?}", body);
    Ok((headers, body))
}
