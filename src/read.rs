use std::{io::{self, Read}, path::Path, time::Instant};
use mio::net::TcpStream;
use crate::cgi::run_cgi;
use crate::handler::*;
use crate::{config::Route, utils::{HttpHeaders, session::handle_session}};
use crate::response::{HttpResponseBuilder, handle_method_not_allowed};
use crate::{config::ServerConfig, models::{HttpResponseCommon, SimpleResponse}, request::{HttpRequest, ParserState}, server::{ListenerInfo, SocketData, SocketStatus, Status}, utils::{HttpMethod, cookie::Cookie}};

fn resolve_file_path(
    server: &ServerConfig,
    route: &crate::config::Route,
    request_path: &str,
) -> Option<String> {
    println!(
        "Resolving file path for request_path: '{}' under route: '{}'",
        request_path, route.path
    );
    let server_root = &server.root;
    let route_root = &route.root;
    let base = format!("{}/{}", server_root, route_root);

    let base_path = match Path::new(&base).canonicalize() {
        Ok(path) => path,
        Err(_) => return None,
    };

    let relative_path = request_path
        .strip_prefix(&route.path)
        .unwrap_or("")
        .trim_start_matches('/');

    let full_path = base_path.join(relative_path);
    let canonical = match full_path.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            let parent = full_path.parent()?;
            let canonical_parent = parent.canonicalize().ok()?;
            if !canonical_parent.starts_with(&base_path) {
                return None;
            }
            full_path
        }
    };

    if canonical.starts_with(&base_path) {
        canonical.to_str().map(|s| s.to_string())
    } else {
        None
    }
}


fn find_matching_route<'a>(server: &'a ServerConfig, request_path: &str) -> Option<&'a Route> {
    server
        .routes
        .iter()
        .filter(|route| {
            if route.path == "/" {
                true
            } else {
                request_path == route.path || request_path.starts_with(&(route.path.clone() + "/"))
            }
        })
        .max_by_key(|route| route.path.len())
}


fn extract_hostname(headers: &HttpHeaders) -> &str {
    headers
        .get("host")
        .and_then(|h| h.split(':').next())
        .unwrap_or("")
}


fn get_error_page_path(server: &ServerConfig, status_code: u16) -> String {
    server
        .error_pages
        .iter()
        .find(|ep| ep.code == status_code)
        .map(|ep| ep.path.clone())
        .unwrap_or_else(|| format!("./error_pages/{}.html", status_code))
}

fn select_server<'a>(listener_info: &'a ListenerInfo, hostname: &str) -> &'a ServerConfig {
    if let Some(srv) = listener_info
        .servers
        .iter()
        .find(|s| s.server_name == hostname)
    {
        println!(
            "Selected server '{}' for Host: {}",
            srv.server_name, hostname
        );
        return srv;
    }

    let default_index = listener_info.default_server_index;
    let default_srv = listener_info.servers.get(default_index).unwrap_or_else(|| {
        panic!(
            "Invalid default_server_index {} for listener with {} servers",
            default_index,
            listener_info.servers.len()
        )
    });

    println!(
        "No match for Host: '{}', using default server '{}'",
        hostname, default_srv.server_name
    );

    default_srv
}

fn read_request(
    stream: &mut TcpStream,
    socket: &mut SocketStatus,
    listener_info: Option<&ListenerInfo>,
) -> Option<bool> {
    let mut buf = [0u8; 4096];

    loop {
        socket.ttl = Instant::now();

        match stream.read(&mut buf) {
            Ok(0) => return None,

            Ok(n) => {
                socket.request.append(buf[..n].to_vec()).ok()?;

                if socket.request.header_done() && !socket.server_selected {
                    println!("hello");
                    let request = socket.request.get_before_done()?;
                    let hostname = extract_hostname(&request.headers);
                    let info = listener_info?;

                    let selected = select_server(info, hostname);
                    socket.max_body_size = Some(selected.client_max_body_size);
                    socket.server_selected = true;
                }

                if let Some(max) = socket.max_body_size {
                    if socket.request.body_len() > max {
                        socket.body_too_large = true;
                        socket.request.set_state(ParserState::Complete);
                        return Some(true);
                    }
                }

                if socket.request.done() {
                    return Some(true);
                }
            }

            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                return Some(false);
            }

            Err(_) => return None,
        }
    }
}


pub fn handle_read_state(
    socket_data: &mut SocketData,
    listener_info: Option<&ListenerInfo>,
) -> Option<bool> {
    let read_result = read_request(
        &mut socket_data.stream,
        &mut socket_data.status,
        listener_info,
    );

    match read_result {
        Some(true) => {}
        other => return other,
    }

    let request: &HttpRequest = socket_data.status.request.get()?;

    // handle cookies and sessions
    let cookie: Cookie = handle_session(request, &mut socket_data.session_store);

    // Select server based on Host header
    let hostname = extract_hostname(&request.headers);
    let info = listener_info.expect("No listener info available");
    let selected_server: &ServerConfig = select_server(info, hostname);

    // check if the socket says body too large
    match socket_data.status.body_too_large {
        true => {
            println!(" too large qflksqdjflmqsdkjflqmskdfjlqskdjf");
            // Body is too large → return 413 Payload Too Large
            let response = HttpResponseBuilder::new(413, "Payload Too Large")
                .body(b"Request body too large".to_vec())
                .build();
            socket_data.status.response = Some(Box::new(SimpleResponse::new(response)));
            socket_data.status.status = Status::Write;

            return Some(true);
        }
        false => {
            println!("false false false ")
            // Body size is fine → continue processing
        }
    }

    let selected_route = find_matching_route(selected_server, &request.path);

    if let Some(route) = selected_route {
        if let Some(redirect) = &route.redirect {
            let response_bytes = HttpResponseBuilder::redirect(redirect)
                .cookie(&cookie)
                .build();
            socket_data.status.response = Some(Box::new(SimpleResponse::new(response_bytes)));
        } else {
            let request_method = &request.method;
            let method_allowed = route
                .methods
                .iter()
                .any(|m| HttpMethod::from_str(m) == *request_method);

            if !method_allowed {
                let allowed = &route.methods;
                let response_bytes = handle_method_not_allowed(&allowed, &selected_server, &cookie);
                socket_data.status.response = Some(Box::new(SimpleResponse::new(response_bytes)));
            } else {
                let file_path = resolve_file_path(selected_server, route, &request.path)
                    .unwrap_or_else(|| "".to_string());

                if let Some(cgi_ext) = &route.cgi {
                    if request.path.ends_with(cgi_ext) {
                        let cgi_context = crate::cgi::CgiContext::from_request(request);
                        if run_cgi(route, cgi_context, &file_path, socket_data) {
                            return Some(true);
                        } else {
                            return None;
                        }
                    }
                }

                let response: Box<dyn HttpResponseCommon> = match request_method {
                    HttpMethod::GET => handle_get(&file_path, &selected_server, &request, &cookie),
                    HttpMethod::POST => {
                        let response_bytes = handle_post(&file_path, &request, &cookie);
                        Box::new(SimpleResponse::new(response_bytes))
                    }
                    HttpMethod::DELETE => {
                        let error_path = get_error_page_path(selected_server, 404);
                        let response_bytes = handle_delete(&file_path, &error_path, &cookie);
                        Box::new(SimpleResponse::new(response_bytes))
                    }
                    HttpMethod::Other(_) => {
                        let allowed = &route.methods;
                        let response_bytes =
                            handle_method_not_allowed(&allowed, &selected_server, &cookie);
                        Box::new(SimpleResponse::new(response_bytes))
                    }
                };

                socket_data.status.response = Some(response);
            }
        }
    } else {
        let error_path = get_error_page_path(selected_server, 404);
        let response_bytes =
            HttpResponseBuilder::serve_error_page(&error_path, 404, "Not Found", &cookie);
        socket_data.status.response = Some(Box::new(SimpleResponse::new(response_bytes)));
    }

    socket_data.status.status = Status::Write;
    Some(true)
}