use std::collections::HashMap;

use crate::request::HttpRequest;

pub trait Handler: Send + Sync + 'static {
    fn handle(&self, req: &HttpRequest) -> Vec<u8>;
}
impl<F> Handler for F
where
    F: Fn(&HttpRequest) -> Vec<u8> + Send + Sync + 'static,
{
    fn handle(&self, req: &HttpRequest) -> Vec<u8> {
        (self)(req)
    }
}

pub struct Router {
    routes: HashMap<String, Box<dyn Handler>>,
}

impl Router {
    pub fn new() -> Self {
        Router {
            routes: HashMap::new(),
        }
    }

    pub fn handle<H: Handler>(&mut self, path: &str, handler: H) {
        self.routes.insert(path.to_string(), Box::new(handler));
    }

    pub fn route(&self, path: &str) -> Option<&Box<dyn Handler>> {
        self.routes.get(path)
    }
}
