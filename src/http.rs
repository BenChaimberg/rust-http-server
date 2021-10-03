use std::collections;
use std::io::{Read, Write};
use std::net;
use std::str;
use std::str::FromStr;
use crate::error::Error;

pub const HTTP_VERSION: &str = "HTTP/1.1";
const CRLF: &str = "\r\n";
const BUF_SIZE: usize = 32;

pub trait RequestHandler {
    fn handle<'a>(&self, r: Request) -> Response;
}

pub fn handle_client(mut stream: net::TcpStream, handler: &impl RequestHandler) -> Result<(), Error> {
    let request = parse_request(&mut stream)?;

    /*
    println!("-- request --");
    println!("{:#?}", request);
     */

    let response = handler.handle(request);

    /*
    println!("-- response --");
    println!("{:#?}", response);
     */

    stream.write_all(response.to_string().as_bytes())?;

    Ok(())
}

fn parse_request(stream: &mut net::TcpStream) -> Result<Request, Error> {
    let mut buf = [0; BUF_SIZE];
    let mut request_line: Option<RequestLine> = None;
    let mut body = String::new();
    let mut header_lines = collections::HashMap::new();

    let mut continuation = String::new();
    loop {
        let bytes_read = stream.read(&mut buf)?;
        if bytes_read > 0 {
            continuation.push_str(str::from_utf8(&buf[..bytes_read])?);
            let mut s = continuation.as_str();
            // println!("-- raw request --");
            // println!("{}", s);
            let mut next_break = match s.find(CRLF) {
                None => {
                    continuation = String::from(s);
                    continue
                },
                Some(i) => i,
            };
            // println!("-- first break: {} --", next_break);

            if request_line.is_none() {
                request_line = Some(parse_request_line(&s[..next_break])?);
                s = &s[next_break+2..];
                next_break = match s.find(CRLF) {
                    None => {
                        continuation = String::from(s);
                        continue
                    },
                    Some(i) => i,
                };
            }
            if request_line.is_none() {
                break
            }

            // header lines
            let mut do_continue = false;
            loop {
                let line = &s[..next_break];
                // println!("-- line --");
                // println!("{}", line);
                if line.is_empty() {
                    break
                }
                let header_line = parse_header_line(line)?;
                header_lines.insert(header_line.0, header_line.1);

                s = &s[next_break+2..];
                next_break = match s.find(CRLF) {
                    None => {
                        do_continue = true;
                        break
                    },
                    Some(i) => i,
                };
            }
            if do_continue {
                continuation = String::from(s);
                continue
            }

            // should probably look at content length so we don't get overflowed
            s = &s[next_break+2..];
            body.push_str(s);
            break
        } else {
            break
        }
    }

    // println!("-- request_line: {:?} --", request_line);
    // println!("-- header_lines: {:?} --", header_lines);

    Ok(Request {
        header: RequestHeader {
            request_line: request_line.ok_or(Error::new("Could not parse request line".to_string()))?,
            header_lines
        },
        remote: Remote { addr: stream.peer_addr().unwrap() },
        body,
    })
}

fn parse_header_line(line: &str) -> Result<(HeaderField, String), Error> {
    let fields = line.split_once(":")
        .ok_or(Error::new("Could not parse header line".to_string()))?;
    let field_name = HeaderField::from_str(fields.0)?;
    let field_value = String::from(fields.1.trim());
    let header_line = (field_name, field_value);
    // println!("-- header_line: {:?} --", header_line);
    Ok(header_line)
}

fn parse_request_line(line: &str) -> Result<RequestLine, Error> {
    let mut words = line.split(" ");
    let method = words.next()
        .and_then(|w| match w {
            "GET" => Some(Method::Get),
            "POST" => Some(Method::Post),
            _ => None,
        })
        .ok_or(Error::new("Could not get method from request line".to_string()))?;
    // println!("-- method: {:?} --", method);

    let (request_path, query_string) = words.next()
        .map(|s| s.split_once("?").unwrap_or((s, "")))
        .map(|(s1, s2)| (String::from(s1), String::from(s2)))
        .ok_or(Error::new("Could not get request target from request line".to_string()))?;
    // println!("-- request_target: {:?} --", request_target);

    let http_version = words.next()
        .map(String::from)
        .ok_or(Error::new("Could not get HTTP version from request line".to_string()))?;
    // println!("-- http_version: {:?} --", http_version);

    Ok(RequestLine { method, request_path, query_string, http_version })
}

pub fn error_response<T>(status_code: StatusCode, message: Option<T>) -> Response where T: std::fmt::Display {
    match status_code {
        StatusCode::InternalServerError => {
            let message = message.map(|m| format!("{}", m)).unwrap_or("<unknown>".to_string());
            println!("Internal server error: {}", message);
        },
        _ => {},
    }
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

#[derive(Debug)]
pub struct Response {
    pub header: ResponseHeader,
    pub body: String,
}
impl ToString for Response {
    fn to_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.header.to_string());
        s.push_str(CRLF);
        s.push_str(&self.body);
        s.push_str(CRLF);
        s
    }
}

#[derive(Debug)]
pub struct ResponseHeader {
    pub status_line: StatusLine,
    pub header_lines: Vec<(String, String)>, // convert to a map
}
impl ToString for ResponseHeader {
    fn to_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.status_line.to_string());
        for header_line in self.header_lines.as_slice() {
            s.push_str(header_line.0.trim());
            s.push_str(": ");
            s.push_str(header_line.1.trim());
            s.push_str(CRLF);
        }
        s
    }
}

#[derive(Debug)]
pub struct StatusLine {
    pub http_version: String,
    pub status_code: StatusCode,
}
impl ToString for StatusLine {
    fn to_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.http_version);
        s.push_str(" ");
        s.push_str(&self.status_code.to_string());
        s.push_str(CRLF);
        s
    }
}

#[derive(Debug)]
pub enum StatusCode {
    Ok, NotModified, BadRequest, Forbidden, NotFound, InternalServerError
}
impl ToString for StatusCode {
    fn to_string(&self) -> String {
        match self {
            StatusCode::Ok => "200 OK".to_string(),
            StatusCode::NotModified => "304 Not Modified".to_string(),
            StatusCode::BadRequest => "400 Bad Request".to_string(),
            StatusCode::Forbidden => "403 Forbidden".to_string(),
            StatusCode::NotFound => "404 Not Found".to_string(),
            StatusCode::InternalServerError => "500 Internal Server Error".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Request {
    pub header: RequestHeader,
    pub remote: Remote,
    pub body: String,
}

#[derive(Debug)]
pub struct RequestHeader {
    pub request_line: RequestLine,
    pub header_lines: collections::HashMap<HeaderField, String>,
}

#[derive(Debug,PartialEq,Eq,Hash)]
pub enum HeaderField {
    Host, IfModifiedSince, UserAgent, NotSupported
}
impl FromStr for HeaderField {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "If-Modified-Since" => HeaderField::IfModifiedSince,
            "Host" => HeaderField::Host,
            "User-Agent" => HeaderField::UserAgent,
            _ => HeaderField::NotSupported,
        })
    }
}

#[derive(Debug)]
pub struct RequestLine {
    pub method: Method,
    pub request_path: String,
    pub query_string: String,
    http_version: String,
}

#[derive(Debug)]
pub enum Method {
    Get, Post
}
impl ToString for Method {
    fn to_string(&self) -> String {
        match self {
            Method::Get => "GET".to_string(),
            Method::Post => "POST".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Remote {
    pub addr: net::SocketAddr,
}
