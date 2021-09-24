mod cgi;
mod config;
mod error;
mod host;
mod http;

fn main() -> Result<(), error::Error> {
    let server_config = config::load_config(&std::path::Path::new("/home/accts/bnc24/cs434/projects/p1/server.conf"))?;
    println!("Reserved server configuration:");
    println!("{:#?}", server_config);

    let port = "3333";
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("Listening on port {}...", port);

    // accept connections and process them serially
    for stream in listener.incoming() {
        let handler = host::Host::new(&server_config);
        http::handle_client(stream?, &handler)?;
    }
    Ok(())
}
