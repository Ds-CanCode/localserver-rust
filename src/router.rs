use crate::config::Route;
use crate::request::HttpRequest;
use crate::response::HttpResponse;
use std::collections::HashMap;

pub type Handler = fn(&HttpRequest) -> HttpResponse;

pub struct Router {
    routes: HashMap<String, Handler>,
    config_routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Router {
            routes: HashMap::new(),
            config_routes: Vec::new(),
        }
    }

    pub fn load_routes(&mut self, routes: Vec<Route>) {
        self.config_routes = routes;
    }

    pub fn add_handler(&mut self, path: &str, handler: Handler) {
        self.routes.insert(path.to_string(), handler);
    }

    pub fn route(&self, path: &str) -> Option<Handler> {
        self.routes.get(path).copied()
    }

    pub fn find_route(&self, path: &str) -> Option<&Route> {
        // Find the longest matching route
        self.config_routes
            .iter()
            .filter(|route| path.starts_with(&route.path))
            .max_by_key(|route| route.path.len())
    }

    pub fn handle_request(&self, request: &HttpRequest) -> HttpResponse {
        // First check for custom handlers
        if let Some(handler) = self.route(&request.path) {
            return handler(request);
        }

        // Then check config routes
        if let Some(route) = self.find_route(&request.path) {
            // Check if method is allowed
            if !route.methods.is_empty() 
                && !route.methods.contains(&request.method.to_str().to_string()) {
                return self.method_not_allowed();
            }

            // Handle static file serving
            return self.serve_static(request, route);
        }

        // 404 if no route found
        self.not_found()
    }

    fn serve_static(&self, request: &HttpRequest, route: &Route) -> HttpResponse {
        let root = route.root.as_ref().map(|s| s.as_str()).unwrap_or("public");
        
        // Remove route prefix to get file path
        let file_path = request.path.strip_prefix(&route.path)
            .unwrap_or("/");
        
        let file_path = if file_path.is_empty() || file_path == "/" {
            route.default_file.as_ref()
                .map(|s| s.as_str())
                .unwrap_or("index.html")
        } else {
            file_path.trim_start_matches('/')
        };

        let full_path = format!("{}/{}", root, file_path);
        
        match std::fs::read(&full_path) {
            Ok(contents) => {
                let mut response = HttpResponse::new(200);
                
                // Set content type based on extension
                if let Some(content_type) = get_content_type(&full_path) {
                    response.set_header("Content-Type".to_string(), content_type);
                }
                
                response.set_body(contents);
                response
            }
            Err(_) => self.not_found(),
        }
    }

    fn not_found(&self) -> HttpResponse {
        let mut response = HttpResponse::new(404);
        let body = b"<html><body><h1>404 - Not Found</h1></body></html>".to_vec();
        response.set_body(body);
        response
    }

    fn method_not_allowed(&self) -> HttpResponse {
        let mut response = HttpResponse::new(405);
        let body = b"<html><body><h1>405 - Method Not Allowed</h1></body></html>".to_vec();
        response.set_body(body);
        response
    }
}

fn get_content_type(path: &str) -> Option<String> {
    let extension = path.split('.').last()?;
    
    let content_type = match extension {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "pdf" => "application/pdf",
        _ => return None,
    };
    
    Some(content_type.to_string())
}


//Problem: Your router stores function pointers but doesn't handle routes from config.
// ✅ Added config_routes to store routes from config file
// ✅ Added load_routes() to load routes from config
// ✅ Added handle_request() for complete request handling
// ✅ Added serve_static() for serving files
// ✅ Added find_route() to match request path to config routes
// ✅ Added content type detection
// ✅ Renamed handle() to add_handler() for clarity