use std::fs;

use crate::utils::HttpHeaders;

pub struct HttpResponseBuilder {
    status_code: u16,
    status_text: String,
    headers: HttpHeaders,
    body: Vec<u8>,
}

impl HttpResponseBuilder {
    pub fn new(status_code: u16, status_text: &str) -> Self {
        Self {
            status_code,
            status_text: status_text.to_string(),
            headers: HttpHeaders::new(),
            body: Vec::new(),
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key, value);
        self
    }

    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    pub fn build(mut self) -> Vec<u8> {
        // Auto-add Content-Length if not present
        self.headers
            .insert("Content-Length", &self.body.len().to_string());

        let mut response = format!("HTTP/1.1 {} {}\r\n", self.status_code, self.status_text);

        for (key, value) in self.headers.iter() {
            response.push_str(&format!("{}: {}\r\n", key, value));
        }

        response.push_str("\r\n");

        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }

    // === Convenience methods ===

    pub fn ok() -> Self {
        Self::new(200, "OK")
    }

    pub fn not_found() -> Self {
        Self::new(404, "Not Found")
    }

    pub fn method_not_allowed() -> Self {
        Self::new(405, "Method Not Allowed")
    }

    pub fn no_content() -> Self {
        Self::new(204, "No Content")
    }

    pub fn internal_error() -> Self {
        Self::new(500, "Internal Server Error")
    }

    // === File serving methods ===

    /// Serve a file with automatic content-type detection
    pub fn serve_file(path: &str) -> Result<Vec<u8>, std::io::Error> {
        let content = fs::read(path)?;
        let content_type = detect_content_type(path);

        Ok(Self::ok()
            .header("Content-Type", content_type)
            .body(content)
            .build())
    }

    /// Serve a custom error page or fall back to minimal response
    pub fn serve_error_page(error_page_path: &str, status_code: u16, status_text: &str) -> Vec<u8> {
        match fs::read(error_page_path) {
            Ok(content) => {
                println!(
                    "Serving custom {} error page from: {}",
                    status_code, error_page_path
                );
                Self::new(status_code, status_text)
                    .header("Content-Type", "text/html")
                    .body(content)
                    .build()
            }
            Err(_) => {
                println!(
                    "Error page '{}' not found, sending minimal {} response",
                    error_page_path, status_code
                );
                Self::new(status_code, status_text).build()
            }
        }
    }

    /// Try to serve a file, or serve 404 error page on failure
    pub fn serve_file_or_404(file_path: &str, error_page_path: &str) -> Vec<u8> {
        println!("Attempting to serve file: {}", file_path);

        match Self::serve_file(file_path) {
            Ok(response) => {
                println!("File found, serving 200 OK");
                response
            }
            Err(_) => {
                println!("File not found: {}, serving 404 page", file_path);
                Self::serve_error_page(error_page_path, 404, "Not Found")
            }
        }
    }
}

// Helper function to detect content type from file extension
fn detect_content_type(path: &str) -> &'static str {
    if path.ends_with(".html") || path.ends_with(".htm") {
        "text/html"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".txt") {
        "text/plain"
    } else {
        "application/octet-stream"
    }
}

// === Handler functions for different HTTP methods ===

pub fn handle_get(file_path: &str, error_page_path: &str) -> Vec<u8> {
    HttpResponseBuilder::serve_file_or_404(file_path, error_page_path)
}

pub fn handle_post(file_path: &str, body: &[u8], error_page_path: &str) -> Vec<u8> {
    // Example: Write/append to file
    match fs::write(file_path, body) {
        Ok(_) => {
            println!("POST: Successfully wrote to {}", file_path);
            HttpResponseBuilder::ok()
                .header("Content-Type", "text/plain")
                .body(b"File uploaded successfully".to_vec())
                .build()
        }
        Err(e) => {
            eprintln!("POST: Error writing to {}: {:?}", file_path, e);
            HttpResponseBuilder::internal_error()
                .header("Content-Type", "text/plain")
                .body(format!("Error: {}", e).into_bytes())
                .build()
        }
    }
}

pub fn handle_delete(file_path: &str, error_page_path: &str) -> Vec<u8> {
    match fs::remove_file(file_path) {
        Ok(_) => {
            println!("DELETE: Successfully deleted {}", file_path);
            HttpResponseBuilder::no_content().build()
        }
        Err(_) => {
            println!("DELETE: File not found {}", file_path);
            HttpResponseBuilder::serve_error_page(error_page_path, 404, "Not Found")
        }
    }
}

pub fn handle_method_not_allowed(
    allowed_methods: &[String],
    method_not_allowed_path: &str,
) -> Vec<u8> {
    let allow_header = allowed_methods.join(", ");

    match fs::read(method_not_allowed_path) {
        Ok(content) => HttpResponseBuilder::method_not_allowed()
            .header("Allow", &allow_header)
            .header("Content-Type", "text/html")
            .body(content)
            .build(),
        Err(_) => HttpResponseBuilder::method_not_allowed()
            .header("Allow", &allow_header)
            .header("Content-Type", "text/plain")
            .body(b"Method Not Allowed".to_vec())
            .build(),
    }
}
