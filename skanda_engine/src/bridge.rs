use crate::searcher::Searcher;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use url::Url;

pub struct Bridge {
    searcher: Searcher,
}

impl Bridge {
    pub fn new(searcher: Searcher) -> Self {
        Self { searcher }
    }

    pub fn listen(&self, port: u16) -> std::io::Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
        println!("Skanda Bridge listening on 127.0.0.1:{}", port);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    self.handle_client(stream);
                }
                Err(e) => eprintln!("Connection failed: {}", e),
            }
        }
        Ok(())
    }

    fn handle_client(&self, mut stream: TcpStream) {
        let mut buffer = [0; 1024];
        if let Ok(n) = stream.read(&mut buffer) {
            let request = String::from_utf8_lossy(&buffer[..n]);

            if let Some(line) = request.lines().next() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[0] == "OPTIONS" {
                    let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes());
                    return;
                } else if parts.len() >= 2 && parts[0] == "GET" {
                    let path = parts[1];
                    let url_str = format!("http://localhost{}", path);
                    if let Ok(url) = Url::parse(&url_str) {
                        if url.path() == "/search" {
                            let mut query = String::new();
                            let mut is_fuzzy = false;

                            for (k, v) in url.query_pairs() {
                                if k == "q" {
                                    query = v.into_owned();
                                } else if k == "fuzzy" && v == "true" {
                                    is_fuzzy = true;
                                }
                            }

                            if !query.is_empty() {
                                let results = self.searcher.search(&query, is_fuzzy);

                                if let Ok(json_response) = serde_json::to_string(&results) {
                                    let response = format!(
                                        "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        json_response.len(),
                                        json_response
                                    );
                                    let _ = stream.write_all(response.as_bytes());
                                    return;
                                }
                            }
                        }
                    }
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 NOT FOUND\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
                    );
                }
            }
        }
    }
}
