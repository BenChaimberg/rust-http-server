use std::collections::HashMap;
use mio::{Events, Interest, Poll, Registry, Token};
use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use crate::config;
use crate::error::Error;
use crate::host;
use crate::http;

const SERVER: Token = Token(0);

pub struct EventLoop {
    poll: Poll,
    events: Events,
    event_sources: HashMap<Token, EventSource>,
}

impl EventLoop {
    pub fn new(mut listener: TcpListener, server_config: config::ServerConfig) -> Result<Self, Error> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(128);
        poll.registry().register(&mut listener, SERVER, Interest::READABLE)?;

        let mut event_sources = HashMap::new();
        event_sources.insert(SERVER, EventSource::ListenerEventSource(listener, 0, server_config));

        Ok(EventLoop { poll, events, event_sources })
    }

    pub fn next(&mut self) -> Result<(), Error> {
        self.poll.poll(&mut self.events, None)?;
        for event in self.events.iter() {
            let token = event.token();
            let source = self.event_sources.get_mut(&token).ok_or(Error::new(format!("Could not find handler for token: {}", token.0)))?;
            if let Ok(Some(response)) = source.handle_event(event) {
                match response {
                    HandleEventResponse::NewEventSource(token, mut new_source, interests) => {
                        new_source.register(self.poll.registry(), token, interests)?;
                        self.event_sources.insert(token, new_source);
                    },
                    HandleEventResponse::ModifyInterests(interests) => {
                        source.reregister(self.poll.registry(), token, interests)?;
                    },
                    HandleEventResponse::CloseSource => {
                        source.deregister(self.poll.registry())?;
                        self.event_sources.remove(&token);
                    }
                }
            }
        }
        Ok(())
    }
}

enum HandleEventResponse {
    NewEventSource(Token, EventSource, Interest),
    ModifyInterests(Interest),
    CloseSource,
}

enum EventSource {
    ListenerEventSource(TcpListener, usize, config::ServerConfig),
    StreamEventSource(TcpStream, ConnectionState, host::Host),
}

impl EventSource {
    fn handle_event(&mut self, event: &Event) -> Result<Option<HandleEventResponse>, Error> {
        match self {
            Self::ListenerEventSource(listener, token_counter, server_config) => handle_listener_event(event, listener, token_counter, server_config),
            Self::StreamEventSource(stream, connection_state, request_handler) => handle_stream_event(event, stream, connection_state, request_handler),
        }
    }
    fn register(&mut self, registry: &Registry, token: Token, interests: Interest) -> Result<(), Error> {
        match self {
            Self::ListenerEventSource(listener, _, _) => registry.register(listener, token, interests),
            Self::StreamEventSource(stream, _, _) => registry.register(stream, token, interests),
        }.map_err(|e| e.into())
    }
    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest) -> Result<(), Error> {
        match self {
            Self::ListenerEventSource(listener, _, _) => registry.reregister(listener, token, interests),
            Self::StreamEventSource(stream, _, _) => registry.reregister(stream, token, interests),
        }.map_err(|e| e.into())
    }
    fn deregister(&mut self, registry: &Registry) -> Result<(), Error> {
        match self {
            Self::ListenerEventSource(listener, _, _) => registry.deregister(listener),
            Self::StreamEventSource(stream, _, _) => registry.deregister(stream),
        }.map_err(|e| e.into())
    }
}

fn handle_stream_event(event: &Event, stream: &mut TcpStream, connection_state: &mut ConnectionState, request_handler: &host::Host) -> Result<Option<HandleEventResponse>, Error> {
    match connection_state {
        ConnectionState::Read => {
            if event.is_readable() {
                let request = http::parse_request(stream)?;
                *connection_state = ConnectionState::Write(request_handler.handle(request));
                Ok(Some(HandleEventResponse::ModifyInterests(Interest::WRITABLE)))
            } else {
                Ok(None)
            }
        },
        ConnectionState::Write(response) => {
            if event.is_writable() {
                http::write_response(stream, response.clone())?;
                Ok(Some(HandleEventResponse::CloseSource))
            } else {
                Ok(None)
            }
        },
    }
}

// TODO: see if we can add Handle for async request handling
#[derive(Debug)]
enum ConnectionState {
    Read, Write(http::Response)
}

fn handle_listener_event(_: &Event, listener: &mut TcpListener, token_counter: &mut usize, server_config: &config::ServerConfig) -> Result<Option<HandleEventResponse>, Error> {
    match listener.accept() {
        Ok((stream, _)) => {
            *token_counter += 1;
            let token = Token(*token_counter);
            let stream_source = EventSource::StreamEventSource(stream, ConnectionState::Read, host::Host::new(server_config.clone()));
            Ok(Some(HandleEventResponse::NewEventSource(token, stream_source, Interest::READABLE)))
        },
        Err(e) => if e.kind() != std::io::ErrorKind::WouldBlock {
            Err(e.into())
        } else {
            Ok(None)
        },
    }
}
