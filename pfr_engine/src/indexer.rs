use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write, BufWriter};
use std::path::Path;
use std::sync::{Arc, Mutex};
use crate::compression::encode_inverted_entry;
use crate::thread_pool::ThreadPool;

const BLOCK_SIZE: usize = 64 * 1024; // 64KB blocks

pub struct Indexer {
    inverted_index: HashMap<String, Vec<(u32, Vec<u32>)>>,
    blocks: Vec<(u32, u64, u32)>, 
    files: Vec<String>, 
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
        
        // Collector for thread results
        let all_local_indices = Arc::new(Mutex::new(Vec::<HashMap<String, Vec<(u32, Vec<u32>)>>>::new()));
        let all_local_blocks = Arc::new(Mutex::new(Vec::<Vec<(u32, u64, u32, u32)>>::new()));
        let shared_files = Arc::new(Mutex::new(Vec::<String>::new()));
        
        // Use a central atomic-like counter for block IDs to ensure uniqueness
        let global_block_counter = Arc::new(Mutex::new(0u32));

        for (file_id, path) in paths.into_iter().enumerate() {
            if path.is_dir() { continue; }
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

            let local_indices = Arc::clone(&all_local_indices);
            let local_blocks_collector = Arc::clone(&all_local_blocks);
            let block_counter = Arc::clone(&global_block_counter);
            
            pool.execute(move || {
                let mut local_index: HashMap<String, Vec<(u32, Vec<u32>)>> = HashMap::new();
                let mut local_blocks: Vec<(u32, u64, u32, u32)> = Vec::new();

                if let Ok(mut f) = File::open(&path) {
                    let mut buffer = vec![0; BLOCK_SIZE];
                    let mut offset = 0u64;
                    loop {
                        let bytes_read = match f.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };

                        let block_id = {
                            let mut count = block_counter.lock().unwrap();
                            let id = *count;
                            *count += 1;
                            id
                        };

                        local_blocks.push((file_id, offset, bytes_read as u32, block_id));
                        let text = String::from_utf8_lossy(&buffer[..bytes_read]).to_lowercase();
                        
                        let mut block_terms: HashMap<String, Vec<u32>> = HashMap::new();
                        let mut start = 0;
                        for (idx, c) in text.char_indices() {
                            if !c.is_alphanumeric() {
                                if start < idx {
                                    let word = &text[start..idx];
                                    block_terms.entry(word.to_string()).or_default().push(start as u32);
                                }
                                start = idx + c.len_utf8();
                            }
                        }
                        if start < text.len() {
                            let word = &text[start..];
                            block_terms.entry(word.to_string()).or_default().push(start as u32);
                        }

                        for (word, positions) in block_terms {
                            local_index.entry(word).or_default().push((block_id, positions));
                        }

                        offset += bytes_read as u64;
                    }
                }
                
                local_indices.lock().unwrap().push(local_index);
                local_blocks_collector.lock().unwrap().push(local_blocks);
            });
        }

        drop(pool);

        // Merge local indices
        let mut final_index: HashMap<String, Vec<(u32, Vec<u32>)>> = HashMap::new();
        let local_indices = Arc::try_unwrap(all_local_indices).unwrap().into_inner().unwrap();
        for idx in local_indices {
            for (word, entries) in idx {
                final_index.entry(word).or_default().extend(entries);
            }
        }
        
        // Merge blocks
        let mut final_blocks: Vec<(u32, u64, u32, u32)> = Vec::new();
        let local_blocks_collector = Arc::try_unwrap(all_local_blocks).unwrap().into_inner().unwrap();
        for blocks in local_blocks_collector {
            final_blocks.extend(blocks);
        }
        final_blocks.sort_by_key(|b| b.3);
        
        self.inverted_index = final_index;
        self.blocks = final_blocks.into_iter().map(|(f, o, l, _)| (f, o, l)).collect();
        self.files = Arc::try_unwrap(shared_files).unwrap().into_inner().unwrap();

        for entries in self.inverted_index.values_mut() {
            entries.sort_by_key(|e| e.0);
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
        for (term, block_positions) in &self.inverted_index {
            let bytes = term.as_bytes();
            writer.write_all(&(bytes.len() as u16).to_le_bytes())?;
            writer.write_all(bytes)?;

            let encoded = encode_inverted_entry(block_positions);
            writer.write_all(&(encoded.len() as u32).to_le_bytes())?;
            writer.write_all(&encoded)?;
        }

        writer.flush()?;
        Ok(())
    }
}
