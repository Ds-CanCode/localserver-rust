use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn new(status_code: u16) -> Self {
        let status_text = match status_code {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            413 => "Payload Too Large",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            503 => "Service Unavailable",
            _ => "Unknown",
        }
        .to_string();

        let mut headers = HashMap::new();
        headers.insert("Server".to_string(), "LocalServer/0.1".to_string());
        headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());

        HttpResponse {
            status_code,
            status_text,
            headers,
            body: Vec::new(),
        }
    }

    pub fn set_body(&mut self, body: Vec<u8>) {
        self.headers
            .insert("Content-Length".to_string(), body.len().to_string());
        self.body = body;
    }

    pub fn set_header(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {} {}\r\n",
            self.status_code, self.status_text
        );

        for (key, value) in &self.headers {
            response.push_str(&format!("{}: {}\r\n", key, value));
        }

        response.push_str("\r\n");

        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}


// ✅ HTTP response structure
// ✅ Status codes and text
// ✅ Headers management
// ✅ Conversion to bytes for sending