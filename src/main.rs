mod cgi;
mod config;
mod error;
mod files;
mod host;
mod http;
mod parse;
mod time;

fn main() -> Result<(), error::Error> {
    let server_config = config::load_config(&std::path::Path::new("/home/accts/bnc24/cs434/projects/p1/httpd.conf"))?;
    let port = server_config.directives.get(&config::Directive::ListenPort).ok_or(error::Error::new("Config did not include a ListenPort entry".to_string()))?;
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("Listening on port {}...", port);

    let handler = host::Host::new(&server_config);
    // accept connections and process them serially
    for stream in listener.incoming() {
        if let Err(e) = http::handle_client(stream?, &handler) {
            println!("Encounted error: {:#?}", e);
        }
    }
    Ok(())
}
