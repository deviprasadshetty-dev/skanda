use crate::searcher::Searcher;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use url::Url;

pub struct Bridge {
    searcher: Arc<Searcher>,
}

impl Bridge {
    pub fn new(searcher: Searcher) -> Self {
        Self { searcher: Arc::new(searcher) }
    }

    pub fn listen(&self, port: u16) -> std::io::Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
        println!("Skanda Bridge listening on 127.0.0.1:{}", port);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let searcher = Arc::clone(&self.searcher);
                    std::thread::spawn(move || handle_connection(stream, searcher));
                }
                Err(e) => eprintln!("Connection failed: {}", e),
            }
        }
        Ok(())
    }
}

fn handle_connection(mut stream: TcpStream, searcher: Arc<Searcher>) {
    // Read until the HTTP header terminator \r\n\r\n (dynamic buffer, no 1024 cap)
    let mut buffer: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 512];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buffer.extend_from_slice(&tmp[..n]);
                if buffer.windows(4).any(|w| w == b"\r\n\r\n") { break; }
                if buffer.len() > 8192 { break; } // sanity cap
            }
            Err(_) => return,
        }
    }

    let request = String::from_utf8_lossy(&buffer);

    let first_line = match request.lines().next() {
        Some(l) => l,
        None => return,
    };

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 { return; }

    if parts[0] == "OPTIONS" {
        let _ = stream.write_all(
            b"HTTP/1.1 204 No Content\r\n\
              Access-Control-Allow-Origin: *\r\n\
              Access-Control-Allow-Methods: GET, OPTIONS\r\n\
              Access-Control-Allow-Headers: *\r\n\
              Connection: close\r\n\r\n",
        );
        return;
    }

    if parts[0] != "GET" {
        let _ = stream.write_all(
            b"HTTP/1.1 405 Method Not Allowed\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        );
        return;
    }

    let url_str = format!("http://localhost{}", parts[1]);
    let url = match Url::parse(&url_str) {
        Ok(u) => u,
        Err(_) => {
            let _ = stream.write_all(
                b"HTTP/1.1 400 Bad Request\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
            );
            return;
        }
    };

    if url.path() != "/search" {
        let _ = stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        );
        return;
    }

    let mut query = String::new();
    let mut is_fuzzy = false;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "q"     => query = v.into_owned(),
            "fuzzy" => is_fuzzy = v == "true",
            _       => {}
        }
    }

    if query.is_empty() {
        let body = b"[]";
        let _ = stream.write_all(
            format!(
                "HTTP/1.1 200 OK\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n",
                body.len()
            ).as_bytes(),
        );
        let _ = stream.write_all(body);
        return;
    }

    let results = searcher.search(&query, is_fuzzy);
    match serde_json::to_string(&results) {
        Ok(json) => {
            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n{}",
                json.len(),
                json
            );
            let _ = stream.write_all(response.as_bytes());
        }
        Err(_) => {
            let _ = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
            );
        }
    }
}
