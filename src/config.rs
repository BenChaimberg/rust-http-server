use std::collections::HashMap;
use std::fs;
use std::path;
use std::str::FromStr;
use crate::error::Error;
use crate::parse::{discard_char, discard_string};

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub directives: HashMap<Directive, String>,
    pub virtual_hosts: Vec<VirtualHost>,
}

#[derive(Clone, Debug)]
pub struct VirtualHost {
    pub directives: HashMap<Directive, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Directive {
    CacheSize, DocumentRoot, ListenPort, ServerName, ThreadPoolSize
}
impl FromStr for Directive {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CacheSize" => Ok(Directive::CacheSize),
            "DocumentRoot" => Ok(Directive::DocumentRoot),
            "Listen" => Ok(Directive::ListenPort),
            "ServerName" => Ok(Directive::ServerName),
            "ThreadPoolSize" => Ok(Directive::ThreadPoolSize),
            _ => Err(())
        }
    }
}

pub fn load_config(config_path: &path::Path) -> Result<ServerConfig, Error> {
    let s = fs::read_to_string(config_path)?;

    parse_server_config(&s)
}

fn parse_server_config(mut s: &str) -> Result<ServerConfig, Error> {
    let mut directives = HashMap::new();
    let mut virtual_hosts = vec!();
    while !s.is_empty() {
        if let Ok(rest) = discard_char('\n', s) {
            s = rest;
        } else if let Ok(((directive_field, directive_value), rest)) = parse_directive(s) {
            directives.insert(directive_field, directive_value);
            s = rest;
        } else if let Ok((virtual_host, rest)) = parse_virtual_host(s) {
            virtual_hosts.push(virtual_host);
            s = rest;
        } else {
            return Err(Error::new("Could not parse file content into Apache-style configuration file".to_string()))
        }
    }
    Ok(ServerConfig { directives, virtual_hosts })
}

fn parse_directive(s: &str) -> Result<((Directive, String), &str), ()> {
    let (line, rest) = s.split_once('\n').ok_or(())?;
    let (field, value) = line.trim().split_once(' ').ok_or(())?;
    let directive = Directive::from_str(field)?;
    Ok(((directive, value.to_string()), rest))
}

fn parse_virtual_host(s: &str) -> Result<(VirtualHost, &str), ()> {
    let block_name = "VirtualHost";
    let (_, mut s) = parse_open_block(block_name, s)?;
    let mut directives = HashMap::new();
    loop {
        if let Ok(((directive_field, directive_value), rest)) = parse_directive(s) {
            directives.insert(directive_field, directive_value);
            s = rest;
        } else {
            break;
        }
    }
    let (_, s) = parse_close_block(block_name, s)?;
    Ok((VirtualHost { directives }, s))
}

fn parse_open_block<'a>(block_name: &str, s: &'a str) -> Result<((), &'a str), ()> {
    let (s, rest) = s.split_once('\n').ok_or(())?;
    let s = discard_char('<', s)?;
    let s = discard_string(block_name, s)?;
    if s.ends_with('>') {
        Ok(((), rest))
    } else {
        Err(())
    }
}

fn parse_close_block<'a>(block_name: &str, s: &'a str) -> Result<((), &'a str), ()> {
    let (s, rest) = s.split_once('\n').ok_or(())?;
    let s = discard_char('<', s)?;
    let s = discard_char('/', s)?;
    let s = discard_string(block_name, s)?;
    if s.ends_with('>') {
        Ok(((), rest))
    } else {
        Err(())
    }
}
