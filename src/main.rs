mod cgi;
mod config;
mod error;
mod files;
mod host;
mod http;
mod parse;
mod time;

fn main() -> Result<(), error::Error> {
    let args = std::env::args().collect::<Vec<String>>();
    let config_file_arg = args.get(1).map(|s| s.as_str()).unwrap_or("httpd.conf");
    let config_file = std::env::current_dir()?.join(config_file_arg);
    let server_config = config::load_config(&config_file)?;

    let port = server_config.directives.get(&config::Directive::ListenPort).ok_or(error::Error::new("Config did not include a ListenPort entry".to_string()))?;
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("Listening on port {}...", port);

    let handler = host::Host::new(&server_config);
    // accept connections and process them serially
    for stream in listener.incoming() {
        if let Err(e) = http::handle_client(stream?, &handler) {
            println!("Encountered error: {:#?}", e);
        }
    }
    Ok(())
}
