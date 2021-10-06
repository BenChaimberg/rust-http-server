use std::{sync::mpsc, thread, time::Duration};

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

    let port = server_config.directives.get(&config::Directive::ListenPort).ok_or(Error::new("Config did not include a ListenPort entry".to_string()))?;
    println!("Listening on port {}...", port);
    let listener = mio::net::TcpListener::bind(format!("127.0.0.1:{}", port).parse()?)?;
    let listener_token = mio::Token(0);
    let listener_source = select::EventSource::ListenerEventSource(listener, 0, server_config);
    send_cmd.send(select::Command { execute: Box::new(move || { Ok(Some(select::CommandResponse::NewSource(listener_token, listener_source, mio::Interest::READABLE))) }) });

    let thread_send_cmd = send_cmd.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(2000));
        thread_send_cmd.send(select::Command { execute: Box::new(move || { Ok(Some(select::CommandResponse::CloseSource(listener_token))) }) });
    });

    event_loop_thread.join();
    Ok(())
}
