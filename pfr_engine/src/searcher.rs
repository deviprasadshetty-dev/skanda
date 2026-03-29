use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub struct Searcher {
    inverted_index: HashMap<String, Vec<u32>>,
    blocks: Vec<(u32, u64, u32)>, // (File ID, Offset, Length)
    files: Vec<String>, // File paths
}

pub struct SearchResult {
    pub file_path: String,
    pub snippet: String,
}

impl Searcher {
    pub fn load_from_disk<P: AsRef<Path>>(index_path: P) -> std::io::Result<Self> {
        let mut file = File::open(index_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let mut cursor = 0;

        fn read_u32(buffer: &[u8], cursor: &mut usize) -> u32 {
            let val = u32::from_le_bytes(buffer[*cursor..*cursor+4].try_into().unwrap());
            *cursor += 4;
            val
        }

        // 1. Files
        let num_files = read_u32(&buffer, &mut cursor);
        let mut files = Vec::with_capacity(num_files as usize);
        for _ in 0..num_files {
            let len = u16::from_le_bytes(buffer[cursor..cursor+2].try_into().unwrap()) as usize;
            cursor += 2;
            let path_str = String::from_utf8_lossy(&buffer[cursor..cursor+len]).to_string();
            cursor += len;
            files.push(path_str);
        }

        // 2. Blocks
        let num_blocks = read_u32(&buffer, &mut cursor);
        let mut blocks = Vec::with_capacity(num_blocks as usize);
        for _ in 0..num_blocks {
            let f_id = read_u32(&buffer, &mut cursor);
            let off = u64::from_le_bytes(buffer[cursor..cursor+8].try_into().unwrap());
            cursor += 8;
            let len = read_u32(&buffer, &mut cursor);
            blocks.push((f_id, off, len));
        }

        // 3. Inverted Index
        let num_terms = read_u32(&buffer, &mut cursor);
        let mut inverted_index = HashMap::with_capacity(num_terms as usize);
        for _ in 0..num_terms {
            let term_len = u16::from_le_bytes(buffer[cursor..cursor+2].try_into().unwrap()) as usize;
            cursor += 2;
            let term = String::from_utf8_lossy(&buffer[cursor..cursor+term_len]).to_string();
            cursor += term_len;

            let num_block_ids = read_u32(&buffer, &mut cursor);
            let mut block_ids = Vec::with_capacity(num_block_ids as usize);
            for _ in 0..num_block_ids {
                block_ids.push(read_u32(&buffer, &mut cursor));
            }
            inverted_index.insert(term, block_ids);
        }

        Ok(Self { inverted_index, blocks, files })
    }

    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let query_terms: Vec<String> = query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if query_terms.is_empty() {
            return vec![];
        }

        // Broad matching with fuzzy fallback (Iterative Footprint Expansion)
        // Since LLMs might predict fragile terms, we calculate a score based on how many terms match.
        // Instead of pure AND, we'll do OR and rank by match count.
        let mut block_scores: HashMap<u32, usize> = HashMap::new();

        for term in &query_terms {
            if let Some(blocks) = self.inverted_index.get(term) {
                for &b_id in blocks {
                    *block_scores.entry(b_id).or_insert(0) += 1;
                }
            }
        }

        // Filter blocks that have at least some overlap (e.g. at least 50% of the terms, or at least 1 term if query is small)
        let required_matches = std::cmp::max(1, query_terms.len() / 2);
        
        let mut candidate_blocks: Vec<(u32, usize)> = block_scores.into_iter()
            .filter(|&(_, count)| count >= required_matches)
            .collect();
            
        // Sort by highest score first
        candidate_blocks.sort_by(|a, b| b.1.cmp(&a.1));
        // Take top 20 blocks max to avoid extreme disk reading on vague queries
        let candidate_blocks: Vec<(u32, usize)> = candidate_blocks.into_iter().take(20).collect();

        let mut results = Vec::new();

        for (b_id, _score) in candidate_blocks {
            let (f_id, offset, len) = self.blocks[b_id as usize];
            let path = &self.files[f_id as usize];

            if let Ok(mut f) = File::open(path) {
                if f.seek(SeekFrom::Start(offset)).is_ok() {
                    let mut buffer = vec![0; len as usize];
                    if f.read_exact(&mut buffer).is_ok() {
                        let text = String::from_utf8_lossy(&buffer);
                        let text_lower = text.to_lowercase();
                        
                        // Extract snippet centered around first found term
                        let first_term = &query_terms[0];
                        if let Some(idx) = text_lower.find(first_term) {
                            let start = idx.saturating_sub(100);
                            let end = std::cmp::min(text.len(), idx + 200);
                            
                            // Align to word boundaries
                            let mut snippet = String::new();
                            snippet.push_str("...");
                            snippet.push_str(&text[start..end]);
                            snippet.push_str("...");
                            
                            results.push(SearchResult {
                                file_path: path.clone(),
                                snippet,
                            });
                        }
                    }
                }
            }
        }

        results
    }
}