use std::collections::HashMap;
use std::convert::TryInto;
use std::io::Write;
use std::net::SocketAddr;
use std::str;
use std::str::FromStr;
use mio::net::TcpStream;
use crate::error::Error;

pub const HTTP_VERSION: &str = "HTTP/1.1";
const CRLF: &str = "\r\n";

pub fn write_response(stream: &mut TcpStream, response: Response) -> Result<(), Error> {
    let chunk_len: usize = 1024;
    if response.header.header_lines.get(&ResponseHeaderField::ContentLength)
        .and_then(|content_length| u32::from_str(content_length).ok())
        .map(|content_length| content_length > chunk_len.try_into().unwrap())
        .unwrap_or(true)
    {
        // arbitrarily choose a maximum response body length, after which responses will be encoded using chunked transfer coding
        // this is not necessarily the intended use case for chunked transfer coding, but will serve as a demo
        write_chunked(response, stream, chunk_len)?;
    } else {
        stream.write_all(response.to_string().as_bytes())?;
    }
    Ok(())
}

fn write_chunked(mut response: Response, stream: &mut TcpStream, chunk_len: usize) -> Result<(), Error> {
    response.header.header_lines.remove(&ResponseHeaderField::ContentLength);
    response.header.header_lines.insert(ResponseHeaderField::TransferEncoding, "chunked".to_string());
    stream.write_all(response.header.to_string().as_bytes())?;
    stream.write_all(CRLF.as_bytes())?;

    let mut body_iter = response.body.as_bytes().chunks_exact(chunk_len);
    loop {
        if let Some(chunk) = body_iter.next() {
            write_chunk(chunk, chunk_len, stream)?;
        } else {
            break;
        }
    }
    let remainder = body_iter.remainder();
    if remainder.len() > 0 {
        write_chunk(remainder, remainder.len(), stream)?;
    }
    write_chunk(&[], 0, stream)?;
    Ok(())
}

fn write_chunk(chunk: &[u8], chunk_len: usize, stream: &mut TcpStream) -> Result<(), Error> {
    stream.write_all(format!("{:x}", chunk_len).as_bytes())?;
    stream.write_all(CRLF.as_bytes())?;
    stream.write_all(chunk)?;
    stream.write_all(CRLF.as_bytes())?;
    Ok(())
}

pub fn try_parse_request(latest: &[u8], incremental_request: IncrementalRequest) -> Result<IncrementalRequest, Error> {
    // println!("-- current request: {:#?}", incremental_request);
    match incremental_request {
        IncrementalRequest::None(buf) => {
            // println!("-- incr_req None");
            let combined = [&buf, latest].concat();
            let s = str::from_utf8(&combined)?;
            // println!("-- raw request");
            // println!("{}", s);
            let next_break = match s.find(CRLF) {
                None => {
                    // println!("-- no CRLF");
                    return Ok(IncrementalRequest::None(combined.into_boxed_slice()));
                },
                Some(i) => i,
            };
            // println!("-- first break: {}", next_break);

            let request_line = parse_request_line(&s[..next_break])?;
            // TODO: can I use the same next_break index to just slice the buffer instead of this string magic?
            try_parse_request(&[], IncrementalRequest::RequestLine(request_line, s[next_break+2..].to_owned().into_boxed_str().into_boxed_bytes()))
        },
        IncrementalRequest::RequestLine(request_line, buf) => {
            // println!("-- incr_req RequestLine");
            let combined = [&buf, latest].concat();
            let s = str::from_utf8(&combined)?;
            // println!("-- raw request");
            // println!("{}", s);
            let next_break = match s.find(CRLF) {
                None => {
                    // println!("-- no CRLF");
                    return Ok(IncrementalRequest::RequestLine(request_line, combined.into_boxed_slice()));
                },
                Some(i) => i,
            };
            // println!("-- first break: {}", next_break);

            let mut header_lines = HashMap::new();
            let header_line = parse_header_line(&s[..next_break])?;
            header_lines.insert(header_line.0, header_line.1);
            try_parse_request(&[], IncrementalRequest::HeaderLines(request_line, header_lines, s[next_break+2..].to_owned().into_boxed_str().into_boxed_bytes()))
        },
        IncrementalRequest::HeaderLines(request_line, mut header_lines, buf) => {
            // println!("-- incr_req HeaderLines");
            let combined = [&buf, latest].concat();
            let s = str::from_utf8(&combined)?;
            // println!("-- raw request");
            // println!("{}", s);
            let next_break = match s.find(CRLF) {
                None => {
                    // println!("-- no CRLF");
                    return Ok(IncrementalRequest::HeaderLines(request_line, header_lines, combined.into_boxed_slice()));
                },
                Some(i) => i,
            };
            // println!("-- first break: {}", next_break);

            let line = &s[..next_break];
            if line.is_empty() {
                let bytes_left = if let Some(content_len) = header_lines.get(&RequestHeaderField::ContentLength) {
                    usize::from_str(content_len)?
                } else {
                    0
                };
                try_parse_request(&[], IncrementalRequest::Body(request_line, header_lines, String::new(), bytes_left, s[next_break+2..].to_owned().into_boxed_str().into_boxed_bytes()))
            } else {
                let header_line = parse_header_line(line)?;
                header_lines.insert(header_line.0, header_line.1);
                try_parse_request(&[], IncrementalRequest::HeaderLines(request_line, header_lines, s[next_break+2..].to_owned().into_boxed_str().into_boxed_bytes()))
            }
        },
        IncrementalRequest::Body(request_line, header_lines, mut body, mut bytes_left, buf) => {
            // println!("-- incr_req Body");
            let combined = [&buf, latest].concat();
            let s = str::from_utf8(&combined)?;
            // println!("-- raw request");
            // println!("{}", s);
            let bytes_to_add = std::cmp::min(bytes_left, s.len());
            body.push_str(&s[..bytes_to_add]);
            bytes_left -= bytes_to_add;
            if bytes_left == 0 {
                Ok(IncrementalRequest::FullRequest(RequestNoRemote { header: RequestHeader { request_line, header_lines }, body }))
            } else {
                Ok(IncrementalRequest::Body(request_line, header_lines, body, bytes_left, s[bytes_to_add..].to_owned().into_boxed_str().into_boxed_bytes()))
            }
        },
        IncrementalRequest::FullRequest(_) => Err(Error::new("Tried to parse but incremental request was already full".to_string())),
    }
}

fn parse_header_line(line: &str) -> Result<(RequestHeaderField, String), Error> {
    let fields = line.split_once(":")
        .ok_or(Error::new("Could not parse header line".to_string()))?;
    let field_name = RequestHeaderField::from_str(fields.0)?;
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
            header_lines: HashMap::new(),
        },
        body: String::new(),
    }
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct ResponseHeader {
    pub status_line: StatusLine,
    pub header_lines: HashMap<ResponseHeaderField, String>,
}
impl ToString for ResponseHeader {
    fn to_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.status_line.to_string());
        for (field, value) in self.header_lines.iter() {
            s.push_str(&field.to_string());
            s.push_str(": ");
            s.push_str(value.trim());
            s.push_str(CRLF);
        }
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResponseHeaderField {
    ContentLength, ContentType, Date, LastModified, Server, TransferEncoding
}
impl ToString for ResponseHeaderField {
    fn to_string(&self) -> String {
        match self {
            ResponseHeaderField::ContentLength => "Content-Length",
            ResponseHeaderField::ContentType => "Content-Type",
            ResponseHeaderField::Date => "Date",
            ResponseHeaderField::LastModified => "Last-Modified",
            ResponseHeaderField::Server => "Server",
            ResponseHeaderField::TransferEncoding => "Transfer-Encoding",
        }.to_string()
    }
}
impl FromStr for ResponseHeaderField {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Content-Length" => Ok(ResponseHeaderField::ContentLength),
            "Content-Type" => Ok(ResponseHeaderField::ContentType),
            "Date" => Ok(ResponseHeaderField::Date),
            "Last-Modified" => Ok(ResponseHeaderField::LastModified),
            "Server" => Ok(ResponseHeaderField::Server),
            "Transfer-Encoding" => Ok(ResponseHeaderField::TransferEncoding),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub enum StatusCode {
    Ok, NotModified, BadRequest, Forbidden, NotFound, InternalServerError
}
impl ToString for StatusCode {
    fn to_string(&self) -> String {
        match self {
            StatusCode::Ok => "200 OK",
            StatusCode::NotModified => "304 Not Modified",
            StatusCode::BadRequest => "400 Bad Request",
            StatusCode::Forbidden => "403 Forbidden",
            StatusCode::NotFound => "404 Not Found",
            StatusCode::InternalServerError => "500 Internal Server Error",
        }.to_string()
    }
}

#[derive(Debug)]
pub enum IncrementalRequest {
    None(Box<[u8]>),
    RequestLine(RequestLine, Box<[u8]>),
    HeaderLines(RequestLine, HashMap<RequestHeaderField, String>, Box<[u8]>),
    Body(RequestLine, HashMap<RequestHeaderField, String>, String, usize, Box<[u8]>),
    FullRequest(RequestNoRemote),
}

#[derive(Debug)]
pub struct RequestNoRemote {
    pub header: RequestHeader,
    pub body: String,
}

#[derive(Debug)]
pub struct Request {
    pub header: RequestHeader,
    pub remote: Remote,
    pub body: String,
}
impl Request {
    pub fn from_no_remote(request: RequestNoRemote, stream: &TcpStream) -> Self {
        Request { header: request.header, remote: Remote { addr: stream.peer_addr().unwrap() }, body: request.body }
    }
}

#[derive(Debug)]
pub struct RequestHeader {
    pub request_line: RequestLine,
    pub header_lines: HashMap<RequestHeaderField, String>,
}

#[derive(Debug,PartialEq,Eq,Hash)]
pub enum RequestHeaderField {
    ContentLength, Host, IfModifiedSince, UserAgent, NotSupported
}
impl FromStr for RequestHeaderField {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Content-Length" => RequestHeaderField::ContentLength,
            "If-Modified-Since" => RequestHeaderField::IfModifiedSince,
            "Host" => RequestHeaderField::Host,
            "User-Agent" => RequestHeaderField::UserAgent,
            _ => RequestHeaderField::NotSupported,
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

#[derive(Debug,PartialEq)]
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
    pub addr: SocketAddr,
}
