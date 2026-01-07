use std::{fs::File, io::{self, BufReader, Read}};

use crate::{response::detect_content_type, utils::cookie::Cookie};
pub trait HttpResponseCommon {
    fn peek(&self) -> &[u8];
    fn next(&mut self, n: usize);
    fn is_finished(&self) -> bool;
    fn fill_if_needed(&mut self) -> io::Result<()>;
}

pub struct SimpleResponse {
    data: Vec<u8>,
    index: usize,
}

impl SimpleResponse {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, index: 0 }
    }
}

impl HttpResponseCommon for SimpleResponse {
    fn peek(&self) -> &[u8] {
        &self.data[self.index..]
    }

    fn next(&mut self, n: usize) {
        self.index += n;
    }

    fn is_finished(&self) -> bool {
        self.index >= self.data.len()
    }
    fn fill_if_needed(&mut self) -> io::Result<()> {
        Ok(())
    } // no-op
}

pub struct FileResponse {
    headers: Vec<u8>,
    headers_index: usize,
    headers_sent: bool,
    reader: BufReader<File>,
    buffer: [u8; 8192],
    buf_len: usize,
    buf_index: usize,
    finished: bool,
}

impl FileResponse {
    pub fn new(file_path: &str, cookie: &Cookie) -> io::Result<Self> {
        let content_type = detect_content_type(file_path);
        let file = File::open(file_path)?;
        let metadata = file.metadata()?;

        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\nSet-Cookie: {}\r\n\r\n",
            metadata.len(),
            content_type,
            cookie.to_header_value()
        )
        .into_bytes();

        Ok(Self {
            headers,
            headers_sent: false,
            headers_index: 0,
            reader: BufReader::new(file),
            buffer: [0; 8192],
            buf_len: 0,
            buf_index: 0,
            finished: false,
        })
    }

    /// Fill the buffer if it's empty
    fn fill_buffer(&mut self) -> io::Result<()> {
        if self.buf_index >= self.buf_len && !self.finished {
            let n = self.reader.read(&mut self.buffer)?;
            self.buf_index = 0;
            self.buf_len = n;
            if n == 0 {
                self.finished = true;
            }
        }
        Ok(())
    }
}

impl HttpResponseCommon for FileResponse {
    fn peek(&self) -> &[u8] {
        if !self.headers_sent {
            &self.headers[self.headers_index..]
        } else {
            &self.buffer[self.buf_index..self.buf_len]
        }
    }

    fn next(&mut self, n: usize) {
        if !self.headers_sent {
            self.headers_index += n;
            if self.headers_index >= self.headers.len() {
                self.headers_sent = true;
            }
        } else {
            self.buf_index += n;
        }
    }

    fn is_finished(&self) -> bool {
        self.headers_sent && self.finished && self.buf_index >= self.buf_len
    }

    fn fill_if_needed(&mut self) -> io::Result<()> {
        if self.headers_sent && self.buf_index >= self.buf_len && !self.finished {
            self.fill_buffer()?;
        }
        Ok(())
    }
}