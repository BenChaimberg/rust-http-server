use std::fs;
use std::path;
use std::str::FromStr;
use crate::error::Error;

#[derive(Debug)]
pub struct ServerConfig {
    pub listen_port: u32,
    pub virtual_hosts: Vec<VirtualHost>,
}

#[derive(Debug)]
pub struct VirtualHost {
    pub document_root: path::PathBuf,
    pub server_name: String,
}

pub fn load_config(config_path: &path::Path) -> Result<ServerConfig, Error> {
    fs::read_to_string(config_path)
        .map_err(|e| e.into())
        .and_then(|file_content| {
            file_content.split_once("\n")
                .and_then(|(listen_line, rest)| {
                    parse_listen_line(listen_line).map(|listen_port| (listen_port, rest))
                })
                .and_then(|(listen_port, rest)| {
                    parse_virtual_hosts(rest).map(|virtual_hosts| (listen_port, virtual_hosts))
                })
                .map(|(listen_port, virtual_hosts)| {
                    ServerConfig {
                        listen_port: listen_port,
                        virtual_hosts: virtual_hosts,
                    }
                })
                .ok_or(Error { message: "Could not parse file content into Apache-style configuration file".to_string() })
        })
}

fn parse_listen_line(s: &str) -> Option<u32> {
    let listen_prefix = "Listen ";
    if s.starts_with(listen_prefix) {
        let port = &s[listen_prefix.len()..];
        let parsed = u32::from_str(port);
        parsed.ok()
    } else {
        None
    }
}

fn parse_virtual_hosts(s: &str) -> Option<Vec<VirtualHost>> {
    let mut lines = s.split("\n").map(|line| line.trim()).filter(|line| !line.is_empty());
    let mut virtual_hosts = vec!();
    loop {
        let open_line = lines.next();
        if open_line.is_none() {
            break;
        }
        if !open_line.unwrap().starts_with("<VirtualHost ") || !open_line.unwrap().ends_with(">") {
            return None;
        }

        let mut document_root: Option<path::PathBuf> = None;
        let mut server_name: Option<String> = None;
        loop {
            let line_opt = lines.next();
            if line_opt.is_none() {
                break;
            }
            let line = line_opt.unwrap();

            let document_root_prefix = "DocumentRoot ";
            let server_name_prefix = "ServerName ";

            if line.starts_with(document_root_prefix) {
                if document_root.is_none() {
                    document_root = path::Path::new(&line[document_root_prefix.len()..]).canonicalize().ok();
                } else {
                    return None;
                }
            } else if line.starts_with(server_name_prefix) {
                if server_name.is_none() {
                    server_name = Some(String::from(&line[server_name_prefix.len()..]));
                } else {
                    return None;
                }
            } else if line == "</VirtualHost>" {
                break;
            } else {
                return None;
            }
        }
        let virtual_host = document_root.and_then(|document_root| {
            server_name.map(|server_name| VirtualHost { document_root, server_name })
        });
        if virtual_host.is_none() {
            return None;
        }

        virtual_hosts.push(virtual_host.unwrap())
    }
    if virtual_hosts.is_empty() {
        None
    } else {
        Some(virtual_hosts)
    }
}
