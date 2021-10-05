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

    let (recv_ready, threads) = spawn_threads(&server_config)?;

    for stream in listener.incoming() {
        let pass_to_worker = || -> Result<(), error::Error> {
            let thread_num = recv_ready.recv()?;
            let thread = threads.get(thread_num).ok_or_else(|| error::Error::new("Received out of bounds thread number, somehow...".to_string()))?;
            thread.send_stream.send(stream?)?;
            Ok(())
        };
        if let Err(e) = pass_to_worker() {
            println!("Main: thread error: {}", e);
        }
    }

    Ok(())
}

fn spawn_threads(server_config: &config::ServerConfig) -> Result<(mpsc::Receiver<usize>, Vec<Thread>), error::Error> {
    let num_threads: usize = server_config.directives.get(&config::Directive::ThreadPoolSize)
        .map(|size| usize::from_str(size))
        .unwrap_or(Ok(1))?;
    let mut threads: Vec<Thread> = Vec::with_capacity(num_threads);
    let (send_ready, recv_ready) = mpsc::channel();
    for thread_num in 0..num_threads {
        let (send_stream, recv_stream) = mpsc::channel();
        let send_ready = send_ready.clone();
        let server_config = server_config.clone();
        thread::spawn(move || worker(thread_num, send_ready, recv_stream, &server_config));
        threads.push(Thread { send_stream });
    }
    Ok((recv_ready, threads))
}

fn worker(thread_num: usize, send_ready: mpsc::Sender<usize>, recv_stream: mpsc::Receiver<TcpStream>, server_config: &config::ServerConfig) -> () {
    let handler = host::Host::new(&server_config);
    let do_work = || -> Result<(), error::Error> {
        send_ready.send(thread_num)?;
        let stream = recv_stream.recv()?;
        http::handle_client(stream, &handler)?;
        Ok(())
    };
    loop {
        if let Err(e) = do_work() {
            println!("Worker{}: thread error: {}", thread_num, e);
        }
    }
}
