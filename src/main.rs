mod http;
mod host;

fn main() -> Result<(), http::Error> {
    let port = "3333";
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("Listening on port {}...", port);

    // accept connections and process them serially
    for stream in listener.incoming() {
        let handler = host::Host::new();
        http::handle_client(stream?, &handler)?;
    }
    Ok(())
}
