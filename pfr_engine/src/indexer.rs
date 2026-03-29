use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write, BufWriter};
use std::path::Path;
use std::sync::{Arc, Mutex};
use crate::compression::encode_delta;
use crate::thread_pool::ThreadPool;

const BLOCK_SIZE: usize = 64 * 1024; // 64KB blocks

pub struct Indexer {
    inverted_index: HashMap<String, Vec<u32>>,
    blocks: Vec<(u32, u64, u32)>, // (File ID, Offset, Length)
    files: Vec<String>, // File paths
}

impl Indexer {
    pub fn new() -> Self {
        Self {
            inverted_index: HashMap::new(),
            blocks: Vec::new(),
            files: Vec::new(),
        }
    }

    pub fn index_directory<P: AsRef<Path>>(&mut self, dir_path: P) {
        let entries = match fs::read_dir(dir_path) {
            Ok(e) => e,
            Err(_) => return,
        };

        let paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        let pool = ThreadPool::new(8);
        
        let shared_index = Arc::new(Mutex::new(HashMap::<String, Vec<u32>>::new()));
        let shared_blocks = Arc::new(Mutex::new(Vec::::<(u32, u64, u32, u32)>::new())); // (file_id, offset, len, block_id)
        let shared_files = Arc::new(Mutex::new(Vec::<String>::new()));

        for (file_id, path) in paths.into_iter().enumerate() {
            if path.is_dir() {
                // Recursion skipped for simplicity in parallel version, or handled differently
                continue;
            }
            if !path.is_file() { continue; }
            
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
            if !matches!(ext.as_deref(), Some("txt") | Some("md") | Some("csv") | Some("json") | Some("rs") | Some("py") | Some("js")) {
                continue;
            }

            let file_id = file_id as u32;
            let path_str = path.to_string_lossy().to_string();
            {
                let mut files = shared_files.lock().unwrap();
                files.push(path_str);
            }

            let shared_index = Arc::clone(&shared_index);
            let shared_blocks = Arc::clone(&shared_blocks);
            
            pool.execute(move || {
                if let Ok(mut f) = File::open(&path) {
                    let mut buffer = vec![0; BLOCK_SIZE];
                    let mut offset = 0u64;
                    loop {
                        let bytes_read = match f.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };

                        // We need a globally unique block_id.
                        // For simplicity in parallel, we'll store local results and merge later,
                        // or use a central counter.
                        let mut blocks = shared_blocks.lock().unwrap();
                        let block_id = blocks.len() as u32;
                        blocks.push((file_id, offset, bytes_read as u32, block_id));
                        drop(blocks);

                        let text = String::from_utf8_lossy(&buffer[..bytes_read]);
                        let mut unique_words = HashSet::new();
                        for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()) {
                            unique_words.insert(word.to_string());
                        }

                        let mut index = shared_index.lock().unwrap();
                        for word in unique_words {
                            index.entry(word).or_insert_with(Vec::new).push(block_id);
                        }
                        drop(index);

                        offset += bytes_read as u64;
                    }
                }
            });
        }

        // Wait for all tasks (implicitly on drop of pool)
        drop(pool);

        self.inverted_index = Arc::try_unwrap(shared_index).unwrap().into_inner().unwrap();
        let mut blocks_with_id = Arc::try_unwrap(shared_blocks).unwrap().into_inner().unwrap();
        // Sorting blocks by ID to ensure correct indexing
        blocks_with_id.sort_by_key(|b| b.3);
        self.blocks = blocks_with_id.into_iter().map(|(f, o, l, _)| (f, o, l)).collect();
        self.files = Arc::try_unwrap(shared_files).unwrap().into_inner().unwrap();

        // Sort block IDs within inverted index because they might be added out of order
        for ids in self.inverted_index.values_mut() {
            ids.sort_unstable();
        }
    }

    pub fn save_to_disk<P: AsRef<Path>>(&self, index_path: P) -> std::io::Result<()> {
        let file = File::create(index_path)?;
        let mut writer = BufWriter::new(file);

        writer.write_all(&(self.files.len() as u32).to_le_bytes())?;
        for path_str in &self.files {
            let bytes = path_str.as_bytes();
            writer.write_all(&(bytes.len() as u16).to_le_bytes())?;
            writer.write_all(bytes)?;
        }

        writer.write_all(&(self.blocks.len() as u32).to_le_bytes())?;
        for (f_id, off, len) in &self.blocks {
            writer.write_all(&f_id.to_le_bytes())?;
            writer.write_all(&off.to_le_bytes())?;
            writer.write_all(&len.to_le_bytes())?;
        }

        writer.write_all(&(self.inverted_index.len() as u32).to_le_bytes())?;
        for (term, block_ids) in &self.inverted_index {
            let bytes = term.as_bytes();
            writer.write_all(&(bytes.len() as u16).to_le_bytes())?;
            writer.write_all(bytes)?;

            let encoded = encode_delta(block_ids);
            writer.write_all(&(block_ids.len() as u32).to_le_bytes())?;
            writer.write_all(&(encoded.len() as u32).to_le_bytes())?;
            writer.write_all(&encoded)?;
        }

        writer.flush()?;
        Ok(())
    }
}
