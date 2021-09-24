use std::collections;
use std::io::{Read, Write};
use std::path;
use std::process;
use crate::config::*;
use crate::error::Error;
use crate::http::*;

pub struct Cgi<'a> {
    server_config: &'a ServerConfig,
}
impl<'a> Cgi<'a> {
    pub fn new(server_config: &'a ServerConfig) -> Cgi {
        Cgi { server_config }
    }
}
impl Cgi<'_> {
    pub fn handle(&self, path: path::PathBuf, request: Request, virtual_host: &VirtualHost) -> Response {
        let remote_addr = request.remote.addr.to_string();
        let request_method = request.header.request_line.method.to_string();
        let server_port = self.server_config.listen_port.to_string();
        let envs: collections::HashMap<&str, &str> = [
            ("QUERY_STRING", request.header.request_line.query_string.as_str()),
            ("REMOTE_ADDR", &remote_addr),
            // ("REMOTE_HOST", ""), NULL if not provided
            // ("REMOTE_IDENT", ""), MAY
            // ("REMOTE_USER", ""), MUST if AUTH_TYPE is Basic or Digest
            ("REQUEST_METHOD", &request_method),
            ("SERVER_NAME", &virtual_host.server_name),
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
            .unwrap_or_else(|e| {
                println!("-- bad cgi --");
                error_response(StatusCode::InternalServerError, Some(e))
            })
    }
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
        .ok_or(Error { message: "Could not parse headers from CGI response".to_string() })?;
    let body = sections.next().unwrap_or("").to_string();
    // println!("-- cgi parsed --");
    // println!("{:?}", headers);
    // println!("{:?}", body);
    Ok((headers, body))
}
