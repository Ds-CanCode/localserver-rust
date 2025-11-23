pub mod cgi;
pub mod config;
pub mod error;
pub mod request;
pub mod response;
pub mod router;
pub mod server;
pub mod utils;

use server::Server;

fn main() {
    println!(" Starting LocalServer...");

    // Load configuration
    let config = match config::load_config("config.yaml") {
        Ok(config) => {
            println!("Configuration loaded successfully!");
            config
        }
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            return;
        }
    };

    // Initialize server
    let mut server = match Server::new() {
        Ok(server) => {
            println!("Server initialized");
            server
        }
        Err(e) => {
            eprintln!("Failed to initialize server: {}", e);
            return;
        }
    };

    // Run server
    println!("\n Server is ready to accept connections");
    if let Err(e) = server.run(config) {
        eprintln!("Server error: {}", e);
    }
}