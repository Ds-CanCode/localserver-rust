use crate::config::Config;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::io::{self, Read};

const SERVER_TOKEN: Token = Token(0);

pub struct Server {
    poll: Poll,
    events: Events,
    listeners: HashMap<Token, TcpListener>,
    connections: HashMap<Token, TcpStream>,
    next_token: usize,
}

impl Server {
    pub fn new() -> io::Result<Self> {
        Ok(Server {
            poll: Poll::new()?,
            events: Events::with_capacity(1024),
            listeners: HashMap::new(),
            connections: HashMap::new(),
            next_token: 1, // Start tokens for connections from 1
        })
    }

    pub fn run(&mut self, config: Config) -> io::Result<()> {
        // For now, let's use the first server config and first port
        let server_config = &config.servers[0];
        let addr = format!("{}:{}", server_config.host, server_config.ports[0])
            .parse()
            .unwrap();

        let mut main_listener = TcpListener::bind(addr)?;
        self.poll
            .registry()
            .register(&mut main_listener, SERVER_TOKEN, Interest::READABLE)?;
        self.listeners.insert(SERVER_TOKEN, main_listener);

        println!("Server listening on {}", addr);

        loop {
            self.poll.poll(&mut self.events, None)?;

            for event in self.events.iter() {
                match event.token() {
                    SERVER_TOKEN => {
                        // Accept new connections
                        loop {
                            match self.listeners.get_mut(&SERVER_TOKEN).unwrap().accept() {
                                Ok((mut stream, _)) => {
                                    let token = Token(self.next_token);
                                    self.next_token += 1;

                                    self.poll.registry().register(
                                        &mut stream,
                                        token,
                                        Interest::READABLE,
                                    )?;
                                    self.connections.insert(token, stream);
                                    println!("Accepted new connection with token: {:?}", token);
                                }
                                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    // No more connections to accept
                                    break;
                                }
                                Err(e) => {
                                    eprintln!("Failed to accept connection: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    token => {
                        if event.is_readable() {
                            if let Some(stream) = self.connections.get_mut(&token) {
                                let mut buf = [0u8; 1024];
                                loop {
                                    match stream.read(&mut buf) {
                                        Ok(0) => {
                                            // Connection closed by client
                                            println!("Client {:?} disconnected", token);
                                            self.poll.registry().deregister(stream)?;
                                            self.connections.remove(&token);
                                            break;
                                        }
                                        Ok(n) => {
                                            // You successfully read `n` bytes
                                            let received = &buf[..n];
                                            println!(
                                                "Received from {:?}: {:?} ",
                                                token,
                                                String::from_utf8_lossy(received)
                                            );

                                            // Here you could parse the message or respond
                                        }
                                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                            // No more data to read
                                            break;
                                        }
                                        Err(e) => {
                                            eprintln!("Read error on {:?}: {}", token, e);
                                            self.poll.registry().deregister(stream)?;
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
        }
    }
}
