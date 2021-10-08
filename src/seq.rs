use std::io::{Read, Write};

use crate::error::Error;
use crate::host;
use crate::http;

pub fn process(request_handler: &host::Host, mut stream: std::net::TcpStream) -> Result<(), Error> {
    let mut buf = [0; 32];
    let mut buf_str = String::new();
    let mut incremental_request = http::IncrementalRequest::None(Box::new([]));
    loop {
        let bytes_read = match stream.read(&mut buf) {
            Ok(bytes_read) => {
                // println!("-- read {} bytes", bytes_read);
                if bytes_read > 0 {
                    buf_str.push_str(std::str::from_utf8(&buf[..bytes_read])?);
                    bytes_read
                } else {
                    break;
                }
            },
            Err(e) => return Err(e.into()),
        };
        // println!("-- current buffer: {}", buf_str);

        incremental_request = http::try_parse_request(&buf[..bytes_read], incremental_request)?;

        if matches!(incremental_request, http::IncrementalRequest::FullRequest(_)) {
            // println!("-- full request: {:#?}", incremental_request);
            break;
        }
    }
    // println!("-- worker {}: finished read stream", thread_num);

    if let http::IncrementalRequest::FullRequest(request) = incremental_request {
        let response = request_handler.handle(&http::Request::from_no_remote(request, stream.peer_addr()?));
        stream.write_all(&http::write_response(response)?)?;
        Ok(())
    } else {
        Err(Error::new("Could not parse a full request using all available data".to_string()))
    }
}
