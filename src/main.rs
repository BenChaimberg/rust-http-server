use std::{ops::Deref, sync::mpsc, thread};

use crate::error::Error;

mod cgi;
mod config;
mod error;
mod files;
mod host;
mod http;
mod parse;
mod pool;
mod select;
mod seq;
mod time;

#[derive(Debug)]
enum MultiModel {
    Single, ThreadPool, SelectMultiplex
}

fn main() -> Result<(), Error> {
    let args = std::env::args().collect::<Vec<String>>();
    let config_file_arg = args.get(1).map(|s| s.as_str()).unwrap_or("httpd.conf");
    let config_file = std::env::current_dir()?.join(config_file_arg);
    let server_config = config::load_config(&config_file)?;

    let multi_model = args.get(2)
        .and_then(|s| match s.deref() {
            "single" => Some(MultiModel::Single),
            "pool" => Some(MultiModel::ThreadPool),
            "select" => Some(MultiModel::SelectMultiplex),
            _ => None,
        })
        .unwrap_or(MultiModel::Single);
    println!("Chose concurrency model: {:#?}", multi_model);

    match multi_model {
        MultiModel::Single => single(server_config),
        MultiModel::ThreadPool => thread_pool(server_config),
        MultiModel::SelectMultiplex => select_multiplex(server_config),
    }
}

fn single(server_config: config::ServerConfig) -> Result<(), Error> {
    let port = server_config.directives.get(&config::Directive::ListenPort).ok_or(Error::new("Config did not include a ListenPort entry".to_string()))?;
    println!("Listening on port {}...", port);
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port))?;

    let request_handler = host::Host::new(server_config);

    for stream in listener.incoming() {
        if let Err(e) = seq::process(&request_handler, stream?, false) {
            println!("Error processing request: {}", e);
        }
    }
    Ok(())
}

fn select_multiplex(server_config: config::ServerConfig) -> Result<(), Error> {
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
    send_cmd.send(Box::new(move |_| { Ok(Some(select::CommandResponse::NewSource(listener_token, listener_source, mio::Interest::READABLE))) }))?;

    println!("Type 'shutdown' to close the listener.");
    let stdin = std::io::stdin();
    let stdin_source = select::EventSource::Stdin(select::Stdin::new(stdin), listener_token);
    send_cmd.send(Box::new(move |_| { Ok(Some(select::CommandResponse::NewSource(stdin_token, stdin_source, mio::Interest::READABLE))) }))?;

    event_loop_thread.join().map_err(|_| Error::new("Event loop thread panicked".to_string()))?
}

fn thread_pool(server_config: config::ServerConfig) -> Result<(), Error> {
    let port = server_config.directives.get(&config::Directive::ListenPort).ok_or(Error::new("Config did not include a ListenPort entry".to_string()))?;
    println!("Listening on port {}...", port);
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port))?;

    let (send_ready, recv_ready, threads) = pool::spawn_threads(&server_config)?;
    for stream in listener.incoming() {
        // println!("-- main: accepted new stream");
        let pass_to_worker = || -> Result<(), error::Error> {
            let thread_num = recv_ready.recv()?;

            let mut ready = Vec::new();
            for thread in recv_ready.try_iter() {
                ready.push(thread);
            }
            let overloaded = ready.len() == 0;
            for thread in ready.drain(..) {
                send_ready.send(thread)?;
            }

            let thread = threads.get(thread_num).ok_or_else(|| error::Error::new("Received out of bounds thread number, somehow...".to_string()))?;
            thread.send_stream.send((stream?, overloaded))?;
            Ok(())
        };
        if let Err(e) = pass_to_worker() {
            println!("Main: thread error: {}", e);
        }
    }
    Ok(())
}
