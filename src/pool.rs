use std::str::FromStr;
use std::sync::mpsc;
use std::thread;

use crate::config;
use crate::error::Error;
use crate::host;
use crate::seq;

pub struct Thread {
    pub send_stream: mpsc::Sender<(std::net::TcpStream, bool)>,
}

pub fn spawn_threads(server_config: &config::ServerConfig) -> Result<(mpsc::Sender<usize>, mpsc::Receiver<usize>, Vec<Thread>), Error> {
    let num_threads: usize = server_config.directives.get(&config::Directive::ThreadPoolSize)
        .map(|size| usize::from_str(size))
        .unwrap_or(Ok(1))?;
    let mut threads: Vec<Thread> = Vec::with_capacity(num_threads);
    let (send_ready, recv_ready) = mpsc::channel();
    for thread_num in 0..num_threads {
        let (send_stream, recv_stream) = mpsc::channel();
        let send_ready = send_ready.clone();
        let server_config = server_config.clone();
        thread::spawn(move || worker(thread_num, send_ready, recv_stream, server_config));
        threads.push(Thread { send_stream });
    }
    Ok((send_ready, recv_ready, threads))
}

fn worker(thread_num: usize, send_ready: mpsc::Sender<usize>, recv_stream: mpsc::Receiver<(std::net::TcpStream, bool)>, server_config: config::ServerConfig) -> () {
    let request_handler = host::Host::new(server_config);
    let do_work = || -> Result<(), Error> {
        // println!("-- worker {}: ready", thread_num);
        send_ready.send(thread_num)?;
        let (stream, overloaded) = recv_stream.recv()?;
        // println!("-- worker {}: received stream", thread_num);
        seq::process(&request_handler, stream, overloaded)
    };
    loop {
        if let Err(e) = do_work() {
            println!("Worker{}: thread error: {}", thread_num, e);
        }
    }
}
