use std::{io, net::Shutdown, time::Instant};
use std::io::{Write};
use crate::{models::HttpResponseCommon, request::HttpRequestBuilder, server::{SocketData, Status}};

fn should_keep_alive(request: &crate::request::HttpRequest) -> bool {
    request
        .headers
        .get("connection")
        .map(|v| v.to_lowercase() == "keep-alive")
        .unwrap_or(false)
}

fn write_response(socket: &mut SocketData) -> Option<bool> {
    let response: &mut Box<dyn HttpResponseCommon + 'static> = socket.status.response.as_mut()?;

    response.fill_if_needed().ok()?;

    let data = response.peek();

    if data.is_empty() {
        return Some(true);
    }
    match socket.stream.write(data) {
        Ok(n) => {
            response.next(n);
            if n > 0 {
                socket.status.ttl = Instant::now();
            }
            if response.is_finished() {
                Some(false)
            } else {
                Some(true)
            }
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Some(false),
        Err(_) => None,
    }
}

pub fn handle_write_state(socket_data: &mut SocketData) -> Option<bool> {
    let write_result = write_response(socket_data);

    match write_result {
        Some(true) => {}
        other => {
            return other;
        }
    }
    let response = socket_data.status.response.as_ref()?;

    if !response.is_finished() {
        println!("Response not finished yet.");
        return Some(true);
    }

    let request = socket_data.status.request.get()?;
    let keep_alive = should_keep_alive(request);

    if keep_alive {
        socket_data.status.status = Status::Read;
        socket_data.status.request = HttpRequestBuilder::new();
        socket_data.status.response = None;
        println!("Keeping connection alive for next request.");
        Some(true)
    } else {
        println!("Closing connection.");
        let _ = socket_data.stream.shutdown(Shutdown::Both);
        None
    }
}
