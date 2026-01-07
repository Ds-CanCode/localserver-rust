use crate::config::{Config, ServerConfig};
use crate::models::HttpResponseCommon;
use crate::read::handle_read_state;
use crate::request::HttpRequestBuilder;
use crate::utils::session::SessionStore;
use crate::write::handle_write_state;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::io::{self};
use std::net::Shutdown;
use std::time::{Duration, Instant};

const LISTENER_TOKEN_START: usize = 0;
const CONNECTION_TOKEN_START: usize = 10000;

#[derive(PartialEq, Debug)]
pub enum Status {
    Read,
    Write,
    Finish,
}

pub struct SocketStatus {
    pub ttl: Instant,
    pub status: Status,
    pub request: HttpRequestBuilder,
    pub response: Option<Box<dyn HttpResponseCommon>>,
    pub server_selected: bool,
    pub body_too_large: bool,
    pub max_body_size: Option<usize>,
}

pub struct SocketData {
    pub stream: TcpStream,
    pub status: SocketStatus,
    pub listener_token: Token,
    pub session_store: SessionStore,
}

pub struct ListenerInfo {
    pub listener: TcpListener,
    pub host: String,
    pub port: u16,
    pub servers: Vec<ServerConfig>,
    pub default_server_index: usize,
}

pub struct Server {
    poll: Poll,
    events: Events,
    listeners: HashMap<Token, ListenerInfo>,
    connections: HashMap<Token, SocketData>,
    session_store: SessionStore,
    next_token: usize,
}

impl Server {
    pub fn new() -> io::Result<Self> {
        Ok(Server {
            poll: Poll::new()?,
            events: Events::with_capacity(1024),
            listeners: HashMap::new(),
            connections: HashMap::new(),
            session_store: SessionStore::new(),
            next_token: CONNECTION_TOKEN_START,
        })
    }

    pub fn run(&mut self, config: Config) -> io::Result<()> {
        let mut listener_map: HashMap<(String, u16), Vec<(usize, ServerConfig)>> = HashMap::new();

        for (idx, server) in config.servers.iter().enumerate() {
            for &port in &server.ports {
                let key = (server.host.clone(), port);
                listener_map
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push((idx, server.clone()));
            }
        }

        let mut token_counter = LISTENER_TOKEN_START;

        for ((host, port), server_list) in listener_map {
            println!("Setting up listener on {}:{}... ", host, port);
            let addr = format!("{}:{}", host, port).parse().unwrap();
            let mut listener = TcpListener::bind(addr)?;
            let token = Token(token_counter);
            token_counter += 1;

            self.poll
                .registry()
                .register(&mut listener, token, Interest::READABLE)?;

            let default_idx = server_list
                .iter()
                .position(|(_, srv)| srv.default_server)
                .unwrap_or(0);

            let servers: Vec<ServerConfig> = server_list.into_iter().map(|(_, srv)| srv).collect();

            println!(
                "Listening on {}:{} with {} server(s)",
                host,
                port,
                servers.len()
            );
            for (i, srv) in servers.iter().enumerate() {
                println!(
                    "  - {} {}",
                    srv.server_name,
                    if i == default_idx { "(default)" } else { "" }
                );
            }

            self.listeners.insert(
                token,
                ListenerInfo {
                    listener,
                    host,
                    port,
                    servers,
                    default_server_index: default_idx,
                },
            );
        }

        loop {
            self.session_store.cleanup();
            self.check_timeouts();
            let timeout = Some(Duration::from_millis(100)); // wait max 100ms

            self.poll.poll(&mut self.events, timeout)?;

            for event in self.events.iter() {
                let token = event.token();

                if token.0 < CONNECTION_TOKEN_START {
                    if let Some(listener_info) = self.listeners.get_mut(&token) {
                        loop {
                            match listener_info.listener.accept() {
                                Ok((mut stream, _)) => {
                                    let conn_token = Token(self.next_token);
                                    self.next_token += 1;

                                    self.poll
                                        .registry()
                                        .register(
                                            &mut stream,
                                            conn_token,
                                            Interest::READABLE.add(Interest::WRITABLE),
                                        )
                                        .unwrap();

                                    self.connections.insert(
                                        conn_token,
                                        SocketData {
                                            stream,
                                            status: SocketStatus {
                                                ttl: Instant::now(),
                                                status: Status::Read,
                                                request: HttpRequestBuilder::new(),
                                                response: None,
                                                server_selected: false,
                                                max_body_size: None,
                                                body_too_large: false,
                                            },
                                            listener_token: token,
                                            session_store: self.session_store.clone(),
                                        },
                                    );

                                    println!(
                                        "Accepted connection {:?} from listener {:?}",
                                        conn_token, token
                                    );
                                }
                                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    break;
                                }
                                Err(e) => {
                                    eprintln!("Accept error: {:?}", e);
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    if let Some(socket_data) = self.connections.get_mut(&token) {
                        loop {
                            let listener_info = self.listeners.get(&socket_data.listener_token);
                            match Server::handle(socket_data, listener_info) {
                                Some(true) => {
                                    continue;
                                }
                                Some(false) => {
                                    break;
                                }
                                None => {
                                    let _ = socket_data.stream.shutdown(Shutdown::Both);
                                    self.connections.remove(&token);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn handle(
        socket_data: &mut SocketData,
        listener_info: Option<&ListenerInfo>,
    ) -> Option<bool> {
        match socket_data.status.status {
            Status::Read => handle_read_state(socket_data, listener_info),
            Status::Write => handle_write_state(socket_data),
            Status::Finish => None,
        }
    }

    fn check_timeouts(&mut self) {
        const TIMEOUT: Duration = Duration::from_secs(5);
        let now = Instant::now();
        let mut expired = Vec::new();

        for (token, conn) in &self.connections {
            if now.duration_since(conn.status.ttl) > TIMEOUT {
                expired.push(*token);
            }
        }

        for token in expired {
            if let Some(mut conn) = self.connections.remove(&token) {
                let _ = self.poll.registry().deregister(&mut conn.stream);
                let _ = conn.stream.shutdown(Shutdown::Both);
            }
        }
    }
}
