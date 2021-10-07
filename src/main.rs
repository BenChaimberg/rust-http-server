use std::{sync::mpsc, thread};

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

    let (send_cmd, recv_cmd) = mpsc::channel::<select::Command>();
    let mut event_loop = select::EventLoop::new(select::CommandQueue::new(send_cmd.clone(), recv_cmd))?;
    println!("Starting event loop...");
    let event_loop_thread = thread::spawn(move || { event_loop.run() });

    let stdin_token = mio::Token(0);
    let listener_token = mio::Token(1);
    let token_counter = 1;

    let port = server_config.directives.get(&config::Directive::ListenPort).ok_or(Error::new("Config did not include a ListenPort entry".to_string()))?;
    println!("Listening on port {}...", port);
    let listener = mio::net::TcpListener::bind(format!("127.0.0.1:{}", port).parse()?)?;
    let listener_source = select::EventSource::TcpListener(listener, token_counter, server_config);
    send_cmd.send(Box::new(move || { Ok(Some(select::CommandResponse::NewSource(listener_token, listener_source, mio::Interest::READABLE))) }))?;

    println!("Type 'shutdown' to close the listener.");
    let stdin = std::io::stdin();
    let stdin_source = select::EventSource::Stdin(select::Stdin::new(stdin), listener_token);
    send_cmd.send(Box::new(move || { Ok(Some(select::CommandResponse::NewSource(stdin_token, stdin_source, mio::Interest::READABLE))) }))?;

    event_loop_thread.join().map_err(|_| Error::new("Event loop thread panicked".to_string()))??;
    Ok(())
}
