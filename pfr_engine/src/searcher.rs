use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use crate::simd_search::find_substring;
use crate::bitset::BitSet;
use crate::compression::decode_inverted_entry;

pub struct Searcher {
    inverted_index: HashMap<String, HashMap<u32, Vec<u32>>>,
    blocks: Vec<(u32, u64, u32)>, 
    files: Vec<String>, 
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

        let num_files = read_u32(&buffer, &mut cursor);
        let mut files = Vec::with_capacity(num_files as usize);
        for _ in 0..num_files {
            let len = u16::from_le_bytes(buffer[cursor..cursor+2].try_into().unwrap()) as usize;
            cursor += 2;
            let path_str = String::from_utf8_lossy(&buffer[cursor..cursor+len]).to_string();
            cursor += len;
            files.push(path_str);
        }

        let num_blocks = read_u32(&buffer, &mut cursor);
        let mut blocks = Vec::with_capacity(num_blocks as usize);
        for _ in 0..num_blocks {
            let f_id = read_u32(&buffer, &mut cursor);
            let off = u64::from_le_bytes(buffer[cursor..cursor+8].try_into().unwrap());
            cursor += 8;
            let len = read_u32(&buffer, &mut cursor);
            blocks.push((f_id, off, len));
        }

        let num_terms = read_u32(&buffer, &mut cursor);
        let mut inverted_index = HashMap::with_capacity(num_terms as usize);
        for _ in 0..num_terms {
            let term_len = u16::from_le_bytes(buffer[cursor..cursor+2].try_into().unwrap()) as usize;
            cursor += 2;
            let term = String::from_utf8_lossy(&buffer[cursor..cursor+term_len]).to_string();
            cursor += term_len;

            let encoded_len = read_u32(&buffer, &mut cursor) as usize;
            let decoded = decode_inverted_entry(&buffer[cursor..cursor+encoded_len]);
            cursor += encoded_len;
            
            let mut block_map = HashMap::with_capacity(decoded.len());
            for (b_id, positions) in decoded {
                block_map.insert(b_id, positions);
            }
            inverted_index.insert(term, block_map);
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

        // Calculate IDF for each query term
        let total_blocks = self.blocks.len() as f32;
        let mut term_idfs = HashMap::new();
        for term in &query_terms {
            let df = self.inverted_index.get(term).map(|m| m.len()).unwrap_or(0) as f32;
            let idf = (total_blocks / (df + 1.0)).ln();
            term_idfs.insert(term.clone(), idf);
        }

        let mut all_candidate_blocks = HashMap::new();
        for term in &query_terms {
            if let Some(block_map) = self.inverted_index.get(term) {
                for &b_id in block_map.keys() {
                    all_candidate_blocks.entry(b_id).or_insert(0);
                }
            }
        }

        let mut block_scores: Vec<(u32, f32)> = Vec::new();

        for &b_id in all_candidate_blocks.keys() {
            let (_, _, block_len) = self.blocks[b_id as usize];
            let mut term_bitsets: Vec<BitSet> = Vec::new();
            
            let mut block_total_idf = 0.0;
            let mut found_count = 0;
            for term in &query_terms {
                let mut bs = BitSet::new(block_len as usize);
                if let Some(block_map) = self.inverted_index.get(term) {
                    if let Some(positions) = block_map.get(&b_id) {
                        for &pos in positions {
                            bs.set(pos as usize);
                        }
                        let idf = term_idfs[term];
                        block_total_idf += idf;
                        found_count += 1;
                    }
                }
                term_bitsets.push(bs);
            }

            if found_count == 0 { continue; }

            // Score based on Proximity + IDF
            let mut density_mask = term_bitsets[0].clone();
            density_mask.proximity_expand(250); 
            
            let mut proximity_score = 1.0;
            for i in 1..term_bitsets.len() {
                if !term_bitsets[i].is_empty() {
                    let mut intersection = term_bitsets[i].clone();
                    for (w_res, w_mask) in intersection.words.iter_mut().zip(density_mask.words.iter()) {
                        *w_res &= *w_mask;
                    }
                    
                    if !intersection.is_empty() {
                        proximity_score += 1.0;
                    } else {
                        proximity_score += 0.3; // Less weight if far
                    }
                    
                    let mut next_dilation = term_bitsets[i].clone();
                    next_dilation.proximity_expand(250);
                    for (w_res, w_mask) in density_mask.words.iter_mut().zip(next_dilation.words.iter()) {
                        *w_res |= *w_mask;
                    }
                }
            }
            
            // Final score: (Sum of IDFs) * Proximity Multiplier
            let final_score = block_total_idf * proximity_score;
            
            block_scores.push((b_id, final_score));
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
                        let text_lower = text.to_lowercase();
                        
                        let mut snippet_pos = None;
                        for term in &query_terms {
                            if let Some(idx) = find_substring(&text_lower, term) {
                                snippet_pos = Some(idx);
                                break;
                            }
                        }

                        if let Some(idx) = snippet_pos {
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
