use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write, BufWriter};
use std::path::Path;
use std::sync::atomic::Ordering;
use rayon::prelude::*;
use crate::compression::encode_inverted_entry;
use crate::SkandaError;

pub struct Indexer {
    inverted_index: HashMap<String, Vec<(u32, Vec<u32>)>>,
    blocks: Vec<(u32, u64, u32)>, 
    files: Vec<String>, 
    allowed_extensions: Vec<String>,
}

impl Indexer {
    pub fn new() -> Self {
        Self {
            inverted_index: HashMap::new(),
            blocks: Vec::new(),
            files: Vec::new(),
            allowed_extensions: vec![
                "txt".into(), "md".into(), "csv".into(), "json".into(), 
                "rs".into(), "py".into(), "js".into()
            ],
        }
    }

    pub fn set_extensions(&mut self, extensions: Vec<String>) {
        self.allowed_extensions = extensions;
    }

    pub fn index_directory<P: AsRef<Path>>(&mut self, dir_path: P) {
        let entries = match fs::read_dir(dir_path) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.retain(|p| {
            if p.is_dir() || !p.is_file() { return false; }
            if let Some(ext) = p.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                self.allowed_extensions.contains(&ext_str)
            } else {
                false
            }
        });

        self.files = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
        let global_block_counter = std::sync::atomic::AtomicU32::new(0);

        let results: Vec<_> = paths.into_par_iter().enumerate().map(|(file_id, path)| {
            let file_id = file_id as u32;
            let mut local_index: HashMap<String, Vec<(u32, Vec<u32>)>> = HashMap::new();
            let mut local_blocks: Vec<(u32, u64, u32, u32)> = Vec::new();

            if let Ok(f) = File::open(&path) {
                let mut reader = BufReader::new(f);
                let mut offset = 0u64;
                let mut line = String::new();
                
                while let Ok(bytes_read) = reader.read_line(&mut line) {
                    if bytes_read == 0 { break; }
                    
                    let block_id = global_block_counter.fetch_add(1, Ordering::Relaxed);
                    local_blocks.push((file_id, offset, bytes_read as u32, block_id));
                    
                    let text = line.to_lowercase();
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
                    line.clear();
                }
            }
            (local_index, local_blocks)
        }).collect();

        let mut final_index: HashMap<String, Vec<(u32, Vec<u32>)>> = HashMap::new();
        let mut final_blocks: Vec<(u32, u64, u32, u32)> = Vec::new();

        for (local_idx, local_blk) in results {
            for (word, entries) in local_idx {
                final_index.entry(word).or_default().extend(entries);
            }
            final_blocks.extend(local_blk);
        }

        final_blocks.sort_by_key(|b| b.3);
        
        self.inverted_index = final_index;
        self.blocks = final_blocks.into_iter().map(|(f, o, l, _)| (f, o, l)).collect();

        for entries in self.inverted_index.values_mut() {
            entries.sort_by_key(|e| e.0);
        }
    }

    pub fn save_to_disk<P: AsRef<Path>>(&self, index_path: P) -> Result<(), SkandaError> {
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
