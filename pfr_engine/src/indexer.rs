use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write, BufWriter};
use std::path::Path;
use crate::compression::encode_delta;

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
        let mut file_id = self.files.len() as u32;

        let entries = match fs::read_dir(dir_path) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.index_directory(&path);
            } else if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if ["txt", "md", "csv", "json", "rs", "py", "js"].contains(&ext_str.as_str()) {
                        self.index_file(file_id, &path);
                        self.files.push(path.to_string_lossy().to_string());
                        file_id += 1;
                    }
                }
            }
        }
    }

    fn index_file(&mut self, file_id: u32, path: &Path) {
        let mut f = match File::open(path) {
            Ok(file) => file,
            Err(_) => return,
        };

        let mut buffer = vec![0; BLOCK_SIZE];
        let mut offset = 0u64;

        loop {
            let bytes_read = match f.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };

            let block_id = self.blocks.len() as u32;
            self.blocks.push((file_id, offset, bytes_read as u32));

            let text = String::from_utf8_lossy(&buffer[..bytes_read]);
            
            let mut unique_words = HashSet::new();
            for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()) {
                unique_words.insert(word.to_string());
            }

            for word in unique_words {
                self.inverted_index.entry(word).or_insert_with(Vec::new).push(block_id);
            }

            offset += bytes_read as u64;
        }
    }

    pub fn save_to_disk<P: AsRef<Path>>(&self, index_path: P) -> std::io::Result<()> {
        let file = File::create(index_path)?;
        let mut writer = BufWriter::new(file);

        // 1. Files
        writer.write_all(&(self.files.len() as u32).to_le_bytes())?;
        for path_str in &self.files {
            let bytes = path_str.as_bytes();
            writer.write_all(&(bytes.len() as u16).to_le_bytes())?;
            writer.write_all(bytes)?;
        }

        // 2. Blocks
        writer.write_all(&(self.blocks.len() as u32).to_le_bytes())?;
        for (f_id, off, len) in &self.blocks {
            writer.write_all(&f_id.to_le_bytes())?;
            writer.write_all(&off.to_le_bytes())?;
            writer.write_all(&len.to_le_bytes())?;
        }

        // 3. Compressed Inverted Index
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
