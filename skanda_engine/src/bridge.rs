use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use crate::searcher::Searcher;

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
            
            // Minimal HTTP-like parsing for the query
            // Expecting: GET /search?q=footprint1+footprint2 HTTP/1.1
            if let Some(line) = request.lines().next() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[0] == "GET" {
                    let path = parts[1];
                    if path.starts_with("/search?q=") {
                        let is_fuzzy = path.contains("&fuzzy=true");
                        let q_end = path.find('&').unwrap_or(path.len());
                        let query = &path[10..q_end].replace('+', " ").replace("%20", " ");
                        let results = self.searcher.search(query, is_fuzzy);
                        
                        let mut response_body = String::from("[\n");
                        for (i, res) in results.iter().enumerate() {
                            // Simple JSON manual formatting to avoid dependencies
                            response_body.push_str("  {\n");
                            response_body.push_str(&format!("    \"file\": \"{}\",\n", res.file_path.replace('\\', "/")));
                            // Escape quotes in snippet for valid JSON
                            let escaped_snippet = res.snippet.replace('"', "\\\"");
                            response_body.push_str(&format!("    \"snippet\": \"{}\"\n", escaped_snippet));
                            response_body.push_str("  }");
                            if i < results.len() - 1 {
                                response_body.push_str(",");
                            }
                            response_body.push_str("\n");
                        }
                        response_body.push_str("]");

                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            response_body.len(),
                            response_body
                        );
                        let _ = stream.write_all(response.as_bytes());
                    } else {
                        let _ = stream.write_all(b"HTTP/1.1 404 NOT FOUND\r\n\r\n");
                    }
                }
            }
        }
    }
}
