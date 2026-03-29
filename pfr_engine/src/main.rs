mod indexer;
mod searcher;
mod bridge;
mod simd_search;

use std::env;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: pfr_engine <command> [args]");
        println!("Commands:");
        println!("  index <dir> <index_path.bin>");
        println!("  search <index_path.bin> <query>");
        println!("  serve <index_path.bin> <port>");
        return;
    }

    let command = &args[1];

    match command.as_str() {
        "index" => {
            if args.len() != 4 {
                println!("Usage: pfr_engine index <dir> <index_path.bin>");
                return;
            }
            let dir = &args[2];
            let index_path = &args[3];

            println!("Indexing directory: {}", dir);
            let start = Instant::now();
            let mut idx = indexer::Indexer::new();
            idx.index_directory(dir);
            
            println!("Saving to {}...", index_path);
            if let Err(e) = idx.save_to_disk(index_path) {
                eprintln!("Failed to save index: {}", e);
            } else {
                println!("Indexing completed in {:?}", start.elapsed());
            }
        }
        "search" => {
            if args.len() < 4 {
                println!("Usage: pfr_engine search <index_path.bin> <query...>");
                return;
            }
            let index_path = &args[2];
            let query = args[3..].join(" ");

            println!("Loading index from {}...", index_path);
            let start = Instant::now();
            match searcher::Searcher::load_from_disk(index_path) {
                Ok(s) => {
                    println!("Index loaded in {:?}", start.elapsed());
                    println!("Searching for: '{}'", query);
                    let search_start = Instant::now();
                    let results = s.search(&query);
                    println!("Found {} results in {:?}", results.len(), search_start.elapsed());

                    for (i, res) in results.iter().enumerate() {
                        println!("--- Result {} ---", i + 1);
                        println!("File: {}", res.file_path);
                        println!("Snippet:\n{}\n", res.snippet);
                    }
                }
                Err(e) => eprintln!("Failed to load index: {}", e),
            }
        }
        "serve" => {
            if args.len() != 4 {
                println!("Usage: pfr_engine serve <index_path.bin> <port>");
                return;
            }
            let index_path = &args[2];
            let port = args[3].parse::<u16>().expect("Invalid port");

            println!("Loading index from {}...", index_path);
            match searcher::Searcher::load_from_disk(index_path) {
                Ok(s) => {
                    let b = bridge::Bridge::new(s);
                    if let Err(e) = b.listen(port) {
                        eprintln!("Bridge error: {}", e);
                    }
                }
                Err(e) => eprintln!("Failed to load index: {}", e),
            }
        }
        _ => {
            println!("Unknown command: {}", command);
        }
    }
}
