mod cgi;
mod config;
mod error;
mod files;
mod host;
mod http;

fn main() -> Result<(), error::Error> {
    let server_config = config::load_config(&std::path::Path::new("/home/accts/bnc24/cs434/projects/p1/server.conf"))?;
    let port = server_config.listen_port;
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("Listening on port {}...", port);

    let handler = host::Host::new(&server_config);
    // accept connections and process them serially
    for stream in listener.incoming() {
        http::handle_client(stream?, &handler)?;
    }
    Ok(())
}
