use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::prelude::AsRawFd;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};
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
            let source = self.event_sources.remove(&token).ok_or(Error::new(format!("Could not find handler for token: {}", token.0)))?;
            match source.handle_event(event, token) {
                Ok((new_source, mut responses)) => {
                    self.event_sources.insert(token, new_source);
                    for response in responses.drain(..) {
                        match response {
                            HandleEventResponse::EmptyCommand(command) => self.submit(Box::new(move |_| Ok(Some(command))))?,
                            HandleEventResponse::Command(command) => self.submit(command)?,
                        };
                    }
                },
                Err(e) => {
                    eprintln!("Handler for token {} produced error: {:#?}", token.0, e);
                },
            }
        }

        let mut new_commands: Vec<Command> = Vec::new();
        loop {
            match self.command_queue.try_recv() {
                Ok(command) => {
                    if let Some(new_command) = self.execute_command(command)? {
                        new_commands.push(new_command);
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(_) => {
                    eprintln!("Sending half of command channel has disconnected; this is probably a fatal error but keep processing IO anyway");
                    break;
                },
            };
        }
        for command in new_commands.drain(..) {
            self.submit(command)?;
        }

        Ok(())
    }

    fn execute_command(&mut self, command: Command) -> Result<Option<Command>, Error> {
        match (command)(&self.event_sources) {
            Ok(response) => if let Some(response) = response {
                match response {
                    CommandResponse::NewSource(token, mut source, interests) => {
                        source.register(self.poll.registry(), token, interests)?;
                        self.event_sources.insert(token, source);
                        Ok(None)
                    },
                    CommandResponse::ModifyInterests(token, interests) => {
                        if let Some(source) = self.event_sources.get_mut(&token) {
                            source.reregister(self.poll.registry(), token, interests)?;
                        } else {
                            eprintln!("Could not find source associated with token {}", token.0);
                        }
                        Ok(None)
                    },
                    CommandResponse::CloseSource(token) => {
                        if let Some(mut source) = self.event_sources.remove(&token) {
                            source.deregister(self.poll.registry())?;
                        } else {
                            eprintln!("Source {} has already been closed", token.0)
                        }
                        Ok(None)
                    },
                    CommandResponse::SubmitCommand(command) => Ok(Some(command)),
                }
            } else {
                Ok(None)
            },
            Err(e) => {
                eprintln!("Command produced error: {:#?}", e);
                Ok(None)
            },
        }
    }
}

pub enum CommandResponse {
    NewSource(Token, EventSource, Interest),
    ModifyInterests(Token, Interest),
    CloseSource(Token),
    SubmitCommand(Command),
}
pub type Command = Box<dyn FnOnce(&HashMap<Token, EventSource>) -> Result<Option<CommandResponse>, Error> + Send>;
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
    EmptyCommand(CommandResponse),
    Command(Command)
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
    TcpStream(TcpStream, ConnectionState, host::Host, Instant),
    Stdin(Stdin, Token),
}

impl EventSource {
    fn handle_event(self, event: &Event, token: Token) -> Result<(EventSource, Vec<HandleEventResponse>), Error> {
        match self {
            Self::TcpListener(listener, token_counter, server_config) => handle_listener_event(event, listener, token_counter, server_config),
            Self::TcpStream(stream, connection_state, request_handler, accept_time) => handle_stream_event(event, token, stream, connection_state, request_handler, accept_time),
            Self::Stdin(stdin, listener_token) => handle_stdin_event(event, stdin, listener_token),
        }
    }
    fn register(&mut self, registry: &Registry, token: Token, interests: Interest) -> Result<(), Error> {
        match self {
            Self::TcpListener(listener, _, _) => registry.register(listener, token, interests),
            Self::TcpStream(stream, _, _, _) => registry.register(stream, token, interests),
            Self::Stdin(stdin, _) => registry.register(stdin, token, interests),
        }.map_err(|e| e.into())
    }
    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest) -> Result<(), Error> {
        match self {
            Self::TcpListener(listener, _, _) => registry.reregister(listener, token, interests),
            Self::TcpStream(stream, _, _, _) => registry.reregister(stream, token, interests),
            Self::Stdin(stdin, _) => registry.reregister(stdin, token, interests),
        }.map_err(|e| e.into())
    }
    fn deregister(&mut self, registry: &Registry) -> Result<(), Error> {
        match self {
            Self::TcpListener(listener, _, _) => registry.deregister(listener),
            Self::TcpStream(stream, _, _, _) => registry.deregister(stream),
            Self::Stdin(stdin, _) => registry.deregister(stdin),
        }.map_err(|e| e.into())
    }
}

fn handle_stream_event(event: &Event, token: Token, mut stream: TcpStream, connection_state: ConnectionState, request_handler: host::Host, accept_time: Instant) -> Result<(EventSource, Vec<HandleEventResponse>), Error> {
    match connection_state {
        ConnectionState::Read(mut incremental_request) => {
            if event.is_readable() {
                let mut buf = [0; 32];
                let mut buf_str = String::new();
                loop {
                    let bytes_read = match stream.read(&mut buf) {
                        Ok(bytes_read) => {
                            // println!("-- read {} bytes", bytes_read);
                            if bytes_read > 0 {
                                buf_str.push_str(std::str::from_utf8(&buf[..bytes_read])?);
                                bytes_read
                            } else {
                                break;
                            }
                        },
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // println!("-- read would block");
                            return Ok((EventSource::TcpStream(stream, ConnectionState::Read(incremental_request), request_handler, accept_time), vec!()));
                        },
                        Err(e) => return Err(e.into()),
                    };
                    // println!("-- current buffer: {}", buf_str);

                    incremental_request = match http::try_parse_request(&buf[..bytes_read], incremental_request) {
                        Ok(incremental_request) => incremental_request,
                        Err(_) => return Ok((
                            EventSource::TcpStream(stream, ConnectionState::Read(http::IncrementalRequest::None(Box::new([]))), request_handler, accept_time),
                            vec!(HandleEventResponse::EmptyCommand(CommandResponse::CloseSource(token))))
                        ),
                    };
                    // println!("-- request after try_parse: {:#?}", incremental_request);

                    if matches!(incremental_request, http::IncrementalRequest::FullRequest(_)) {
                        break;
                    }
                }

                if let http::IncrementalRequest::FullRequest(request) = incremental_request {
                    let response = request_handler.handle(&http::Request::from_no_remote(request, &stream));
                    Ok((
                        EventSource::TcpStream(stream, ConnectionState::Write(http::IncrementalResponse::Struct(response)), request_handler, accept_time),
                        vec!(HandleEventResponse::EmptyCommand(CommandResponse::ModifyInterests(token, Interest::WRITABLE))))
                    )
                } else {
                    Ok((EventSource::TcpStream(stream, ConnectionState::Read(incremental_request), request_handler, accept_time), vec!()))
                }
            } else {
                Ok((EventSource::TcpStream(stream, ConnectionState::Read(incremental_request), request_handler, accept_time), vec!()))
            }
        },
        ConnectionState::Write(mut response) => {
            if event.is_writable() {
                loop {
                    response = match response {
                        http::IncrementalResponse::Struct(response_struct) => http::IncrementalResponse::Bytes(http::write_response(response_struct)?),
                        http::IncrementalResponse::Bytes(bytes) => {
                            match stream.write(&bytes) {
                                Ok(bytes_written) => if bytes_written > 0 {
                                    http::IncrementalResponse::Bytes(Box::from(&bytes[bytes_written..]))
                                } else {
                                    http::IncrementalResponse::Done
                                },
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    return Ok((
                                        EventSource::TcpStream(stream, ConnectionState::Write(http::IncrementalResponse::Bytes(bytes)), request_handler, accept_time),
                                        vec!()
                                    ));
                                },
                                Err(e) => return Err(e.into()),
                            }
                        }
                        http::IncrementalResponse::Done => {
                            return Ok((
                                EventSource::TcpStream(stream, ConnectionState::Close, request_handler, accept_time),
                                vec!(HandleEventResponse::EmptyCommand(CommandResponse::CloseSource(token)))
                            ))
                        },
                    }
                }
            } else {
                Ok((EventSource::TcpStream(stream, ConnectionState::Write(response), request_handler, accept_time), vec!()))
            }
        },
        ConnectionState::Close => Ok((EventSource::TcpStream(stream, ConnectionState::Close, request_handler, accept_time), vec!())),
    }
}

// TODO: see if we can add Handle for async request handling
#[derive(Debug)]
pub enum ConnectionState {
    Read(http::IncrementalRequest), Write(http::IncrementalResponse), Close
}

fn handle_listener_event(_: &Event, listener: TcpListener, mut token_counter: usize, server_config: config::ServerConfig) -> Result<(EventSource, Vec<HandleEventResponse>), Error> {
    match listener.accept() {
        Ok((stream, _)) => {
            token_counter += 1;
            let token = Token(token_counter);
            let stream_source = EventSource::TcpStream(
                stream,
                ConnectionState::Read(http::IncrementalRequest::None(Box::new([]))),
                host::Host::new(server_config.clone()),
                Instant::now()
            );
            /*
             * The order of these two commands does matter; the stream source must exist in the event sources map before
             * `check_stream_timeout` is called. Otherwise, the function will assume that the stream source has already
             * been removed from the map due to closure and will stop re-submitting itself.
             */
            Ok((
                EventSource::TcpListener(listener, token_counter, server_config),
                vec!(
                    HandleEventResponse::EmptyCommand(CommandResponse::NewSource(token, stream_source, Interest::READABLE)),
                    HandleEventResponse::Command(Box::new(move |event_sources| check_stream_timeout(token, event_sources)))
                )
            ))
        },
        Err(e) => if e.kind() == std::io::ErrorKind::WouldBlock {
            Ok((EventSource::TcpListener(listener, token_counter, server_config), vec!()))
        } else {
            Err(e.into())
        },
    }
}

const STREAM_TIMEOUT: Duration = Duration::from_secs(60);
fn check_stream_timeout(token: Token, event_sources: &HashMap<Token, EventSource>) -> Result<Option<CommandResponse>, Error> {
    // println!("-- check_timeout");
    let resubmit = Ok(Some(CommandResponse::SubmitCommand(Box::new(move |event_sources| check_stream_timeout(token, event_sources)))));
    if let Some(source) = event_sources.get(&token) {
        if let EventSource::TcpStream(_, connection_state, _, accept_time) = source {
            if let ConnectionState::Read(incremental_request) = connection_state {
                if !matches!(incremental_request, http::IncrementalRequest::FullRequest(_)) {
                    if let Some(duration) = Instant::now().checked_duration_since(*accept_time) {
                        if duration >= STREAM_TIMEOUT {
                            // println!("-- over timeout, requesting to close source");
                            return Ok(Some(CommandResponse::CloseSource(token)));
                        } else {
                            // println!("-- under timeout, resubmitting command");
                            return resubmit;
                        }
                    } else {
                        return resubmit;
                    }
                }
            }
        } else {
            println!("Checking timeout for stream source but event source was a diferent type");
        }
    }
    Ok(None)
}

fn handle_stdin_event(event: &Event, stdin: Stdin, listener_token: Token) -> Result<(EventSource, Vec<HandleEventResponse>), Error> {
    if event.is_readable() {
        let mut input = String::new();
        stdin.raw.read_line(&mut input)?;
        input = input.trim().to_string();
        if input == "shutdown" {
            println!("Closing the listener...");
            return Ok((EventSource::Stdin(stdin, listener_token), vec!(HandleEventResponse::EmptyCommand(CommandResponse::CloseSource(listener_token)))));
        } else {
            println!("Command not recognized. Type 'shutdown' to close the listener.");
        }
    }
    Ok((EventSource::Stdin(stdin, listener_token), vec!()))
}
