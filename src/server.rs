use crate::config::Config;
use crate::request::HttpRequestBuilder;
use crate::router::Router;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::time::Instant;

#[derive(PartialEq, Debug)]
enum Status {
    Read,
    Write,
    Finish,
}

#[derive(Debug)]
struct SocketStatus {
    ttl: Instant,
    status: Status,
    request: HttpRequestBuilder,
    response_bytes: Vec<u8>,
    index_written: usize,
}

#[derive(Debug)]
struct SocketData {
    stream: TcpStream,
    status: Option<SocketStatus>,
}

pub struct Server {
    poll: Poll,
    events: Events,
    listeners: HashMap<Token, TcpListener>,
    connections: HashMap<Token, SocketData>,
    router: Router,
    next_token: usize,
}

impl Server {
    pub fn new() -> io::Result<Self> {
        Ok(Server {
            poll: Poll::new()?,
            events: Events::with_capacity(1024),
            listeners: HashMap::new(),
            connections: HashMap::new(),
            router: Router::new(),
            next_token: 1,
        })
    }

    pub fn run(&mut self, config: Config) -> io::Result<()> {
        // Load routes into router
        if let Some(server_config) = config.servers.first() {
            self.router.load_routes(server_config.routes.clone());
        }

        // Bind to all configured servers and ports
        for server_config in &config.servers {
            for port in &server_config.ports {
                let addr = format!("{}:{}", server_config.host, port)
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("{}", e)))?;

                let mut listener = TcpListener::bind(addr)?;
                let token = Token(self.next_token);
                self.next_token += 1;

                self.poll
                    .registry()
                    .register(&mut listener, token, Interest::READABLE)?;

                self.listeners.insert(token, listener);
                println!("📡 Listening on {}", addr);
            }
        }

        loop {
            // Poll for events
            self.poll.poll(&mut self.events, None)?;

            // Collect events to process (to avoid borrowing issues)
            let events_to_process: Vec<(Token, bool, bool)> = self.events.iter()
                .map(|event| (event.token(), event.is_readable(), event.is_writable()))
                .collect();

            // Process each event
            for (token, is_readable, is_writable) in events_to_process {
                if self.listeners.contains_key(&token) {
                    // Accept new connections
                    self.accept_connections(token)?;
                } else if is_readable {
                    // Handle readable event
                    let needs_write = if let Some(socket_data) = self.connections.get_mut(&token) {
                        match Self::handle_read(socket_data, &self.router) {
                            HandleResult::NeedsWrite => true,
                            HandleResult::KeepAlive => false,
                            HandleResult::Close => {
                                self.connections.remove(&token);
                                false
                            }
                        }
                    } else {
                        false
                    };

                    // Register for writable if needed
                    if needs_write {
                        if let Some(socket_data) = self.connections.get_mut(&token) {
                            self.poll.registry().reregister(
                                &mut socket_data.stream,
                                token,
                                Interest::WRITABLE,
                            )?;
                        }
                    }
                } else if is_writable {
                    // Handle writable event
                    let result = if let Some(socket_data) = self.connections.get_mut(&token) {
                        Self::handle_write(socket_data)
                    } else {
                        HandleResult::Close
                    };

                    match result {
                        HandleResult::Close => {
                            self.connections.remove(&token);
                        }
                        HandleResult::KeepAlive => {
                            // Re-register for reading
                            if let Some(socket_data) = self.connections.get_mut(&token) {
                                self.poll.registry().reregister(
                                    &mut socket_data.stream,
                                    token,
                                    Interest::READABLE,
                                )?;
                            }
                        }
                        HandleResult::NeedsWrite => {
                            // Stay writable
                        }
                    }
                }
            }

            // Clean up finished connections
            self.connections.retain(|_, socket| {
                socket.status.as_ref()
                    .map(|s| s.status != Status::Finish)
                    .unwrap_or(false)
            });
        }
    }

    fn accept_connections(&mut self, token: Token) -> io::Result<()> {
        if let Some(listener) = self.listeners.get_mut(&token) {
            loop {
                match listener.accept() {
                    Ok((mut stream, addr)) => {
                        let conn_token = Token(self.next_token);
                        self.next_token += 1;

                        self.poll
                            .registry()
                            .register(&mut stream, conn_token, Interest::READABLE)?;

                        let socket_status = SocketStatus {
                            ttl: Instant::now(),
                            status: Status::Read,
                            request: HttpRequestBuilder::new(),
                            response_bytes: Vec::new(),
                            index_written: 0,
                        };

                        let socket_data = SocketData {
                            stream,
                            status: Some(socket_status),
                        };

                        self.connections.insert(conn_token, socket_data);
                        println!("📥 New connection from {} (token: {:?})", addr, conn_token);
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to accept connection: {}", e);
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_read(socket_data: &mut SocketData, router: &Router) -> HandleResult {
        let status = match socket_data.status.as_mut() {
            Some(s) => s,
            None => return HandleResult::Close,
        };

        if status.status != Status::Read {
            return HandleResult::KeepAlive;
        }

        while !status.request.done() {
            let mut buffer = [0; 4096];
            match socket_data.stream.read(&mut buffer) {
                Ok(0) => {
                    println!("🔌 Connection closed by peer");
                    return HandleResult::Close;
                }
                Ok(n) => {
                    status.ttl = Instant::now();
                    
                    match status.request.append(buffer[..n].to_vec()) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("❌ Request parse error: {}", e);
                            let error_response = b"HTTP/1.1 400 Bad Request\r\n\r\nBad Request".to_vec();
                            status.response_bytes = error_response;
                            status.status = Status::Write;
                            return HandleResult::NeedsWrite;
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return HandleResult::KeepAlive;
                }
                Err(e) => {
                    eprintln!("❌ Read error: {:?}", e);
                    return HandleResult::Close;
                }
            }
        }

        // Request complete, generate response
        if let Some(request) = status.request.get() {
            println!("📨 {} {}", request.method.to_str(), request.path);
            
            let response = router.handle_request(&request);
            status.response_bytes = response.to_bytes();
            status.status = Status::Write;
            status.index_written = 0;
            
            return HandleResult::NeedsWrite;
        }

        HandleResult::KeepAlive
    }

    fn handle_write(socket_data: &mut SocketData) -> HandleResult {
        let status = match socket_data.status.as_mut() {
            Some(s) => s,
            None => return HandleResult::Close,
        };

        if status.status != Status::Write {
            return HandleResult::KeepAlive;
        }

        while status.index_written < status.response_bytes.len() {
            match socket_data.stream.write(&status.response_bytes[status.index_written..]) {
                Ok(n) => {
                    status.index_written += n;
                    status.ttl = Instant::now();
                    
                    if status.index_written >= status.response_bytes.len() {
                        println!("✅ Response sent ({} bytes)", status.response_bytes.len());
                        status.status = Status::Finish;
                        return HandleResult::Close;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return HandleResult::NeedsWrite;
                }
                Err(e) => {
                    eprintln!("❌ Write error: {:?}", e);
                    return HandleResult::Close;
                }
            }
        }

        HandleResult::Close
    }
}

enum HandleResult {
    KeepAlive,
    NeedsWrite,
    Close,
}