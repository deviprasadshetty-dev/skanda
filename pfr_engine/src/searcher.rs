use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use crate::simd_search::find_substring;
use crate::bitset::BitSet;
use crate::compression::decode_delta;

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

        // 3. Compressed Inverted Index
        let num_terms = read_u32(&buffer, &mut cursor);
        let mut inverted_index = HashMap::with_capacity(num_terms as usize);
        for _ in 0..num_terms {
            let term_len = u16::from_le_bytes(buffer[cursor..cursor+2].try_into().unwrap()) as usize;
            cursor += 2;
            let term = String::from_utf8_lossy(&buffer[cursor..cursor+term_len]).to_string();
            cursor += term_len;

            let block_ids_count = read_u32(&buffer, &mut cursor);
            let encoded_len = read_u32(&buffer, &mut cursor) as usize;
            let block_ids = decode_delta(&buffer[cursor..cursor+encoded_len], block_ids_count as usize);
            cursor += encoded_len;
            
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

        let mut all_candidate_blocks = HashMap::new();
        for term in &query_terms {
            if let Some(blocks) = self.inverted_index.get(term) {
                for &b_id in blocks {
                    all_candidate_blocks.entry(b_id).or_insert(0);
                }
            }
        }

        let mut block_scores: Vec<(u32, f32)> = Vec::new();

        for &b_id in all_candidate_blocks.keys() {
            let (f_id, offset, len) = self.blocks[b_id as usize];
            let path = &self.files[f_id as usize];
            
            if let Ok(mut f) = File::open(path) {
                if f.seek(SeekFrom::Start(offset)).is_ok() {
                    let mut buffer = vec![0; len as usize];
                    if f.read_exact(&mut buffer).is_ok() {
                        let text = String::from_utf8_lossy(&buffer).to_lowercase();
                        let mut term_bitsets: Vec<BitSet> = Vec::new();
                        
                        let mut found_any = false;
                        for term in &query_terms {
                            let mut bs = BitSet::new(len as usize);
                            let mut search_idx = 0;
                            let mut term_found = false;
                            while let Some(pos) = find_substring(&text[search_idx..], term) {
                                bs.set(search_idx + pos);
                                search_idx += pos + term.len();
                                term_found = true;
                                if search_idx >= text.len() { break; }
                            }
                            if term_found { found_any = true; }
                            term_bitsets.push(bs);
                        }

                        if !found_any { continue; }

                        let mut density_mask = term_bitsets[0].clone();
                        density_mask.proximity_expand(200); 
                        
                        let mut match_count = 1.0;
                        for i in 1..term_bitsets.len() {
                            if !term_bitsets[i].is_empty() {
                                let mut intersection = term_bitsets[i].clone();
                                for (w_res, w_mask) in intersection.words.iter_mut().zip(density_mask.words.iter()) {
                                    *w_res &= *w_mask;
                                }
                                
                                if !intersection.is_empty() {
                                    match_count += 1.0;
                                } else {
                                    match_count += 0.5;
                                }
                                
                                let mut next_dilation = term_bitsets[i].clone();
                                next_dilation.proximity_expand(200);
                                for (w_res, w_mask) in density_mask.words.iter_mut().zip(next_dilation.words.iter()) {
                                    *w_res |= *w_mask;
                                }
                            }
                        }
                        
                        if match_count >= (query_terms.len() as f32 * 0.5) {
                            block_scores.push((b_id, match_count));
                        }
                    }
                }
            }
        }

        block_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let candidate_blocks: Vec<u32> = block_scores.into_iter().take(20).map(|(id, _)| id).collect();

        let mut results = Vec::new();

        for b_id in candidate_blocks {
            let (f_id, offset, len) = self.blocks[b_id as usize];
            let path = &self.files[f_id as usize];

            if let Ok(mut f) = File::open(path) {
                if f.seek(SeekFrom::Start(offset)).is_ok() {
                    let mut buffer = vec![0; len as usize];
                    if f.read_exact(&mut buffer).is_ok() {
                        let text = String::from_utf8_lossy(&buffer);
                        
                        let first_term = &query_terms[0];
                        if let Some(idx) = find_substring(&text.to_lowercase(), first_term) {
                            let start = idx.saturating_sub(100);
                            let end = std::cmp::min(text.len(), idx + 200);
                            
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
