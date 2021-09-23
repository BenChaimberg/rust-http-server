use std::io;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::str;

pub const CRLF: &str = "\r\n";
const BUF_SIZE: usize = 32;

#[derive(Debug)]
pub struct Error {
    message: String,
}
impl std::error::Error for Error {}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.message)?;
        Ok(())
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error { message: format!("IO error: {:?}", e.kind()) }
    }
}
impl From<str::Utf8Error> for Error {
    fn from(_: str::Utf8Error) -> Self {
        Error { message: "Could not interpret a sequence of u8 as a string".to_string() }
    }
}

pub trait RequestHandler {
    fn handle<'a>(&self, r: Request) -> Response;
}

pub fn handle_client(mut stream: TcpStream, handler: &impl RequestHandler) -> Result<(), Error> {
    let request = parse_request(&mut stream)?;

    /*
    println!("-- request --");
    println!("{:#?}", request);
     */

    let response = handler.handle(request);

    println!("-- response --");
    println!("{:#?}", response);

    stream.write_all(response.to_string().as_bytes())?;

    Ok(())
}

fn parse_request(stream: &mut TcpStream) -> Result<Request, Error> {
    let mut buf = [0; BUF_SIZE];
    let mut request_line: Option<RequestLine> = None;
    let mut body = String::new();
    let mut header_lines: Vec<(String, String)> = Vec::new();

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
                let header_line = parse_header_line(line);
                header_lines.push(header_line?);

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
            request_line: request_line.ok_or(Error { message: "Could not parse request line".to_string() })?,
            header_lines
        },
        body,
    })
}

fn parse_header_line(line: &str) -> Result<(String, String), Error> {
    let mut fields = line.split(":");
    let field_name = fields.next()
        .map(String::from)
        .ok_or(Error { message: "Could not get field name from header line".to_string() })?;
    let field_value = fields.next()
        .map(str::trim)
        .map(String::from)
        .ok_or(Error { message: "Could not get field value from header line".to_string() })?;
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
        .ok_or(Error { message: "Could not get method from request line".to_string() })?;
    // println!("-- method: {:?} --", method);

    let request_target = words.next()
        .map(String::from)
        .ok_or(Error { message: "Could not get request target from request line".to_string() })?;
    // println!("-- request_target: {:?} --", request_target);

    let http_version = words.next()
        .map(String::from)
        .ok_or(Error { message: "Could not get HTTP version from request line".to_string() })?;
    // println!("-- http_version: {:?} --", http_version);

    Ok(RequestLine { method, request_target, http_version })
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
    Ok, Forbidden, NotFound, InternalServerError
}
impl ToString for StatusCode {
    fn to_string(&self) -> String {
        match self {
            StatusCode::Ok => "200 OK".to_string(),
            StatusCode::Forbidden => "403 Forbidden".to_string(),
            StatusCode::NotFound => "404 Not Found".to_string(),
            StatusCode::InternalServerError => "500 Internal Server Error".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Request {
    pub header: RequestHeader,
    pub body: String,
}

#[derive(Debug)]
pub struct RequestHeader {
    pub request_line: RequestLine,
    pub header_lines: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct RequestLine {
    pub method: Method,
    pub request_target: String,
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
