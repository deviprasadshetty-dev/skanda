use pfr_engine::{Indexer, Searcher, Bridge};
use std::env;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return;
    }

    let command = &args[1];

    match command.as_str() {
        "index" => {
            if args.len() != 4 {
                println!("Usage: pfr index <dir> <index_path.bin>");
                return;
            }
            let dir = &args[2];
            let index_path = &args[3];

            println!("Indexing directory: {}", dir);
            let start = Instant::now();
            let mut idx = Indexer::new();
            idx.index_directory(dir);
            
            if let Err(e) = idx.save_to_disk(index_path) {
                eprintln!("Failed to save index: {}", e);
            } else {
                println!("Indexing completed in {:?}", start.elapsed());
            }
        }
        "search" => {
            if args.len() < 4 {
                println!("Usage: pfr search <index_path.bin> <query...>");
                return;
            }
            let index_path = &args[2];
            let query = args[3..].join(" ");

            match Searcher::load_from_disk(index_path) {
                Ok(s) => {
                    let results = s.search(&query);
                    
                    if args.contains(&String::from("--json")) {
                        println!("{}", format_results_json(&results));
                    } else {
                        println!("Found {} results", results.len());
                        for (i, res) in results.iter().enumerate() {
                            println!("--- Result {} ---", i + 1);
                            println!("File: {}", res.file_path);
                            println!("Snippet:\n{}\n", res.snippet);
                        }
                    }
                }
                Err(e) => eprintln!("Failed to load index: {}", e),
            }
        }
        "serve" => {
            if args.len() != 4 {
                println!("Usage: pfr serve <index_path.bin> <port>");
                return;
            }
            let index_path = &args[2];
            let port = args[3].parse::<u16>().expect("Invalid port");

            match Searcher::load_from_disk(index_path) {
                Ok(s) => {
                    let b = Bridge::new(s);
                    if let Err(e) = b.listen(port) {
                        eprintln!("Bridge error: {}", e);
                    }
                }
                Err(e) => eprintln!("Failed to load index: {}", e),
            }
        }
        "status" => {
            if args.len() != 3 {
                println!("Usage: pfr status <index_path.bin>");
                return;
            }
            let index_path = &args[2];
            match Searcher::load_from_disk(index_path) {
                Ok(s) => {
                    s.print_status();
                }
                Err(e) => eprintln!("Failed to load index: {}", e),
            }
        }
        _ => {
            println!("Unknown command: {}", command);
            print_usage();
        }
    }
}

fn print_usage() {
    println!("Predictive Footprint Retrieval (PFR) Engine");
    println!("Usage: pfr <command> [args]");
    println!("Commands:");
    println!("  index <dir> <index.bin>      Index a directory of text files");
    println!("  search <index.bin> <query>   Search footprints (use --json for machine output)");
    println!("  serve <index.bin> <port>     Start the JSON HTTP bridge");
    println!("  status <index.bin>           Show index metadata");
}

fn format_results_json(results: &[pfr_engine::SearchResult]) -> String {
    let mut json = String::from("[\n");
    for (i, res) in results.iter().enumerate() {
        json.push_str("  {\n");
        json.push_str(&format!("    \"file\": \"{}\",\n", res.file_path.replace('\\', "/")));
        let escaped = res.snippet.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r");
        json.push_str(&format!("    \"snippet\": \"{}\"\n", escaped));
        json.push_str("  }");
        if i < results.len() - 1 { json.push(','); }
        json.push('\n');
    }
    json.push(']');
    json
}
