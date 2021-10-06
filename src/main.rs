use crate::error::Error;

mod cgi;
mod config;
mod error;
mod files;
mod host;
mod http;
mod parse;
mod select;
mod time;

fn main() -> Result<(), Error> {
    let args = std::env::args().collect::<Vec<String>>();
    let config_file_arg = args.get(1).map(|s| s.as_str()).unwrap_or("httpd.conf");
    let config_file = std::env::current_dir()?.join(config_file_arg);
    let server_config = config::load_config(&config_file)?;

    let port = server_config.directives.get(&config::Directive::ListenPort).ok_or(Error::new("Config did not include a ListenPort entry".to_string()))?;
    let listener = mio::net::TcpListener::bind(format!("127.0.0.1:{}", port).parse()?)?;
    println!("Listening on port {}...", port);

    let mut event_loop = select::EventLoop::new(listener, server_config)?;
    println!("Starting event loop...");

    loop {
        if let Err(e) = event_loop.next() {
            println!("Encountered error in event loop: {:#?}", e);
            break;
        }
    }

    Ok(())
}
