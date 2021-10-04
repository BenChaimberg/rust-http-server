use std::net::TcpStream;
use std::str::FromStr;
use std::sync::mpsc;
use std::thread;

mod cgi;
mod config;
mod error;
mod files;
mod host;
mod http;
mod parse;
mod time;

struct Thread {
    send_stream: mpsc::Sender<TcpStream>,
}

fn main() -> Result<(), error::Error> {
    let args = std::env::args().collect::<Vec<String>>();
    let config_file_arg = args.get(1).map(|s| s.as_str()).unwrap_or("httpd.conf");
    let config_file = std::env::current_dir()?.join(config_file_arg);
    let server_config = config::load_config(&config_file)?;

    let port = server_config.directives.get(&config::Directive::ListenPort).ok_or(error::Error::new("Config did not include a ListenPort entry".to_string()))?;
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("Listening on port {}...", port);

    let num_threads: usize = server_config.directives.get(&config::Directive::ThreadPoolSize).and_then(|size| usize::from_str(size).ok()).unwrap_or(1);
    let mut threads: Vec<Thread> = Vec::with_capacity(num_threads);
    let (send_ready, recv_ready) = mpsc::channel();
    for thread_num in 0..num_threads {
        let send_ready = send_ready.clone();
        let (send_stream, recv_stream) = mpsc::channel();
        let server_config = server_config.clone();
        thread::spawn(move || {
            let handler = host::Host::new(&server_config);
            loop {
                // println!("Worker{}: sending ready signal", thread_num);
                if let Err(e) = send_ready.send(thread_num) {
                    println!("Worker{}: thread error - could not send_ready: {:#?}", thread_num, e);
                }
                // println!("Worker{}: waiting for incoming connection", thread_num);
                match recv_stream.recv() {
                    Ok(stream) => {
                        // println!("Worker{}: received connection, handling", thread_num);
                        if let Err(e) = http::handle_client(stream, &handler) {
                            println!("Worker{}: encountered error during handling: {:#?}", thread_num, e);
                        }},
                    Err(e) => println!("Worker{}: thread error - could not recv_stream: {:#?}", thread_num, e),
                }
            }
        });
        threads.push(Thread { send_stream });
    }

    for stream in listener.incoming() {
        // println!("Main: accepted connection");
        let thread_num = match recv_ready.recv() {
            Ok(thread_num) => thread_num,
            Err(e) => {
                println!("Main: thread error - could not recv_ready: {:#?}", e);
                continue;
            },
        };
        let thread = match threads.get(thread_num) {
            Some(thread) => thread,
            None => {
                println!("Main: thread error - received out of bounds thread_num: {}", thread_num);
                continue;
            },
        };
        if let Err(e) = thread.send_stream.send(stream?) {
            println!("Main: thread error - could not send_stream: {:#?}", e);
        }
        // println!("Main: waiting for incoming connection");
    }

    Ok(())
}
