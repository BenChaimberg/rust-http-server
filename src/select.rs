use std::collections::HashMap;
use std::os::unix::prelude::AsRawFd;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::Duration;
use mio::{Events, Interest, Poll, Registry, Token, event};
use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use mio::unix::SourceFd;
use crate::config;
use crate::error::Error;
use crate::host;
use crate::http;

const POLL_TIMEOUT: Duration = Duration::from_millis(1000);

pub struct EventLoop {
    poll: Poll,
    events: Events,
    event_sources: HashMap<Token, EventSource>,
    command_queue: CommandQueue,
}

impl EventLoop {
    pub fn new(command_queue: CommandQueue) -> Result<Self, Error> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(128);
        let event_sources = HashMap::new();

        Ok(EventLoop { poll, events, event_sources, command_queue })
    }

    pub fn run(&mut self) -> Result<(), Error> {
        loop {
            self.next()?;
        }
    }

    pub fn submit(&self, command: Command) -> Result<(), Error> {
        self.command_queue.send(command)
    }

    fn next(&mut self) -> Result<(), Error> {
        self.poll.poll(&mut self.events, Some(POLL_TIMEOUT))?;

        for event in self.events.iter() {
            let token = event.token();
            let source = self.event_sources.get_mut(&token).ok_or(Error::new(format!("Could not find handler for token: {}", token.0)))?;
            match source.handle_event(event, token) {
                Ok(response) => if let Some(response) = response {
                    match response {
                        HandleEventResponse::Command(command) => self.submit(Box::new(move || Ok(Some(command))))?,
                    };
                },
                Err(e) => {
                    eprintln!("Handler for token {} produced error: {:#?}", token.0, e);
                },
            }
        }

        loop {
            match self.command_queue.try_recv() {
                Ok(command) => self.execute_command(command)?,
                Err(TryRecvError::Empty) => break,
                Err(_) => {
                    eprintln!("Sending half of command channel has disconnected; this is probably a fatal error but keep processing IO anyway");
                    break;
                },
            };
        }

        Ok(())
    }

    fn execute_command(&mut self, command: Command) -> Result<(), Error> {
        match (command)() {
            Ok(response) => if let Some(response) = response {
                match response {
                    CommandResponse::NewSource(token, mut source, interests) => {
                        source.register(self.poll.registry(), token, interests)?;
                        self.event_sources.insert(token, source);
                    },
                    CommandResponse::ModifyInterests(token, interests) => {
                        if let Some(source) = self.event_sources.get_mut(&token) {
                            source.reregister(self.poll.registry(), token, interests)?;
                        } else {
                            eprintln!("Could not find source associated with token {}", token.0);
                        }
                    },
                    CommandResponse::CloseSource(token) => {
                        if let Some(mut source) = self.event_sources.remove(&token) {
                            source.deregister(self.poll.registry())?;
                        } else {
                            eprintln!("Source {} has already been closed", token.0)
                        }
                    },
                };
            },
            Err(e) => {
                eprintln!("Command produced error: {:#?}", e);
            },
        }
        Ok(())
    }
}

pub enum CommandResponse {
    NewSource(Token, EventSource, Interest),
    ModifyInterests(Token, Interest),
    CloseSource(Token),
}
pub type Command = Box<dyn FnOnce() -> Result<Option<CommandResponse>, Error> + Send>;
pub struct CommandQueue {
    send: Sender<Command>,
    recv: Receiver<Command>,
}
impl CommandQueue {
    pub fn new(send: Sender<Command>, recv: Receiver<Command>) -> Self {
        CommandQueue { send, recv }
    }
    pub fn send(&self, command: Command) -> Result<(), Error> {
        self.send.send(command).map_err(|e| e.into())
    }
    pub fn try_recv(&self) -> Result<Command, TryRecvError> {
        self.recv.try_recv()
    }
}

enum HandleEventResponse {
    Command(CommandResponse)
}

pub struct Stdin {
    raw: std::io::Stdin,
}
impl Stdin {
    pub fn new(stdin: std::io::Stdin) -> Self {
        Stdin { raw: stdin }
    }
}
impl event::Source for Stdin {
    fn register(&mut self, registry: &Registry, token: Token, interests: Interest) -> std::io::Result<()> {
        let raw_fd = self.raw.as_raw_fd();
        let mut source_fd = SourceFd(&raw_fd);
        source_fd.register(registry, token, interests)
    }
    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest) -> std::io::Result<()> {
        let raw_fd = self.raw.as_raw_fd();
        let mut source_fd = SourceFd(&raw_fd);
        source_fd.reregister(registry, token, interests)
    }
    fn deregister(&mut self, registry: &Registry) -> std::io::Result<()> {
        let raw_fd = self.raw.as_raw_fd();
        let mut source_fd = SourceFd(&raw_fd);
        source_fd.deregister(registry)
    }
}

pub enum EventSource {
    TcpListener(TcpListener, usize, config::ServerConfig),
    TcpStream(TcpStream, ConnectionState, host::Host),
    Stdin(Stdin, Token),
}

impl EventSource {
    fn handle_event(&mut self, event: &Event, token: Token) -> Result<Option<HandleEventResponse>, Error> {
        match self {
            Self::TcpListener(listener, token_counter, server_config) => handle_listener_event(event, listener, token_counter, server_config),
            Self::TcpStream(stream, connection_state, request_handler) => handle_stream_event(event, token, stream, connection_state, request_handler),
            Self::Stdin(stdin, listener_token) => handle_stdin_event(event, stdin, listener_token),
        }
    }
    fn register(&mut self, registry: &Registry, token: Token, interests: Interest) -> Result<(), Error> {
        match self {
            Self::TcpListener(listener, _, _) => registry.register(listener, token, interests),
            Self::TcpStream(stream, _, _) => registry.register(stream, token, interests),
            Self::Stdin(stdin, _) => registry.register(stdin, token, interests),
        }.map_err(|e| e.into())
    }
    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest) -> Result<(), Error> {
        match self {
            Self::TcpListener(listener, _, _) => registry.reregister(listener, token, interests),
            Self::TcpStream(stream, _, _) => registry.reregister(stream, token, interests),
            Self::Stdin(stdin, _) => registry.reregister(stdin, token, interests),
        }.map_err(|e| e.into())
    }
    fn deregister(&mut self, registry: &Registry) -> Result<(), Error> {
        match self {
            Self::TcpListener(listener, _, _) => registry.deregister(listener),
            Self::TcpStream(stream, _, _) => registry.deregister(stream),
            Self::Stdin(stdin, _) => registry.deregister(stdin),
        }.map_err(|e| e.into())
    }
}

fn handle_stream_event(event: &Event, token: Token, stream: &mut TcpStream, connection_state: &mut ConnectionState, request_handler: &host::Host) -> Result<Option<HandleEventResponse>, Error> {
    match connection_state {
        ConnectionState::Read => {
            if event.is_readable() {
                let request = http::parse_request(stream)?;
                let response = request_handler.handle(request);
                *connection_state = ConnectionState::Write(response);
                Ok(Some(HandleEventResponse::Command(CommandResponse::ModifyInterests(token, Interest::WRITABLE))))
            } else {
                Ok(None)
            }
        },
        ConnectionState::Write(response) => {
            if event.is_writable() {
                http::write_response(stream, response.clone())?;
                Ok(Some(HandleEventResponse::Command(CommandResponse::CloseSource(token))))
            } else {
                Ok(None)
            }
        },
    }
}

// TODO: see if we can add Handle for async request handling
#[derive(Debug)]
pub enum ConnectionState {
    Read, Write(http::Response)
}

fn handle_listener_event(_: &Event, listener: &mut TcpListener, token_counter: &mut usize, server_config: &config::ServerConfig) -> Result<Option<HandleEventResponse>, Error> {
    match listener.accept() {
        Ok((stream, _)) => {
            *token_counter += 1;
            let token = Token(*token_counter);
            let stream_source = EventSource::TcpStream(stream, ConnectionState::Read, host::Host::new(server_config.clone()));
            Ok(Some(HandleEventResponse::Command(CommandResponse::NewSource(token, stream_source, Interest::READABLE))))
        },
        Err(e) => if e.kind() == std::io::ErrorKind::WouldBlock {
            Ok(None)
        } else {
            Err(e.into())
        },
    }
}

fn handle_stdin_event(event: &Event, stdin: &mut Stdin, listener_token: &mut Token) -> Result<Option<HandleEventResponse>, Error> {
    if event.is_readable() {
        let mut input = String::new();
        stdin.raw.read_line(&mut input)?;
        input = input.trim().to_string();
        if input == "shutdown" {
            println!("Closing the listener...");
            return Ok(Some(HandleEventResponse::Command(CommandResponse::CloseSource(*listener_token))));
        } else {
            println!("Command not recognized. Type 'shutdown' to close the listener.");
        }
    }
    Ok(None)
}
