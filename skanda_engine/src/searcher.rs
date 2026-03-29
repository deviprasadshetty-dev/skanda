use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use memmap2::Mmap;
use crate::simd_search::find_substring;
use crate::bitset::BitSet;
use crate::compression::decode_inverted_entry;
use crate::fuzzy_search::{FuzzyMatcher, levenshtein_distance};
use crate::SkandaError;

const MAX_DICT_CAPACITY: usize = 1_000_000;
const PROXIMITY_DILATION: usize = 250;
const PROXIMITY_PENALTY: f32 = 0.3;

pub struct Searcher {
    inverted_index: HashMap<String, HashMap<u32, Vec<u32>>>,
    blocks: Vec<(u32, u64, u32)>, 
    files: Vec<String>, 
}

#[derive(serde::Serialize)]
pub struct SearchResult {
    pub file_path: String,
    pub snippet: String,
}

impl Searcher {
    pub fn load_from_disk<P: AsRef<Path>>(index_path: P) -> Result<Self, SkandaError> {
        let file = File::open(index_path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let buffer = &mmap[..];
        
        let mut cursor = 0;

        fn read_u32(buffer: &[u8], cursor: &mut usize) -> u32 {
            let val = u32::from_le_bytes(buffer[*cursor..*cursor+4].try_into().unwrap());
            *cursor += 4;
            val
        }

        if buffer.len() < 4 { return Err(SkandaError::InvalidIndex); }
        let num_files = read_u32(buffer, &mut cursor);
        let mut files = Vec::with_capacity(num_files.min(MAX_DICT_CAPACITY as u32) as usize);
        for _ in 0..num_files {
            if cursor + 2 > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let len = u16::from_le_bytes(buffer[cursor..cursor+2].try_into().unwrap()) as usize;
            cursor += 2;
            if cursor + len > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let path_str = String::from_utf8_lossy(&buffer[cursor..cursor+len]).to_string();
            cursor += len;
            files.push(path_str);
        }

        if cursor + 4 > buffer.len() { return Err(SkandaError::InvalidIndex); }
        let num_blocks = read_u32(buffer, &mut cursor);
        let mut blocks = Vec::with_capacity(num_blocks.min(MAX_DICT_CAPACITY as u32) as usize);
        for _ in 0..num_blocks {
            if cursor + 16 > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let f_id = read_u32(buffer, &mut cursor);
            let off = u64::from_le_bytes(buffer[cursor..cursor+8].try_into().unwrap());
            cursor += 8;
            let len = read_u32(buffer, &mut cursor);
            blocks.push((f_id, off, len));
        }

        if cursor + 4 > buffer.len() { return Err(SkandaError::InvalidIndex); }
        let num_terms = read_u32(buffer, &mut cursor);
        let mut inverted_index = HashMap::with_capacity(num_terms.min(MAX_DICT_CAPACITY as u32) as usize);
        for _ in 0..num_terms {
            if cursor + 2 > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let term_len = u16::from_le_bytes(buffer[cursor..cursor+2].try_into().unwrap()) as usize;
            cursor += 2;
            if cursor + term_len > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let term = String::from_utf8_lossy(&buffer[cursor..cursor+term_len]).to_string();
            cursor += term_len;

            if cursor + 4 > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let encoded_len = read_u32(buffer, &mut cursor) as usize;
            if cursor + encoded_len > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let decoded = decode_inverted_entry(&buffer[cursor..cursor+encoded_len]);
            cursor += encoded_len;
            
            let mut block_map = HashMap::with_capacity(decoded.len().min(100));
            for (b_id, positions) in decoded {
                block_map.insert(b_id, positions);
            }
            inverted_index.insert(term, block_map);
        }

        Ok(Self { inverted_index, blocks, files })
    }

    pub fn search(&self, query: &str, fuzzy: bool) -> Vec<SearchResult> {
        let query_terms: Vec<String> = query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if query_terms.is_empty() {
            return vec![];
        }

        let mut expanded_terms: HashMap<String, f32> = HashMap::new(); 
        for term in &query_terms {
            expanded_terms.insert(term.clone(), 1.0);
            
            if fuzzy {
                let matcher = FuzzyMatcher::new(term, 1);
                for existing_term in self.inverted_index.keys() {
                    if existing_term.len() < 3 || existing_term == term { continue; }
                    let len_diff = (existing_term.len() as isize - term.len() as isize).abs();
                    if len_diff > 1 { continue; }

                    if matcher.find(existing_term).is_some() {
                        if levenshtein_distance(term, existing_term) == 1 {
                            expanded_terms.entry(existing_term.clone())
                                .and_modify(|e| *e = e.max(0.7))
                                .or_insert(0.7);
                        }
                    }
                }
            }
        }

        let total_blocks = self.blocks.len() as f32;
        let mut term_idfs = HashMap::new();
        for term in expanded_terms.keys() {
            let df = self.inverted_index.get(term).map(|m| m.len()).unwrap_or(0) as f32;
            let idf = (total_blocks / (df + 1.0)).ln();
            term_idfs.insert(term.clone(), idf);
        }

        let mut all_candidate_blocks = HashMap::new();
        for term in expanded_terms.keys() {
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
            
            for term in expanded_terms.keys() {
                let mut bs = BitSet::new(block_len as usize);
                if let Some(block_map) = self.inverted_index.get(term) {
                    if let Some(positions) = block_map.get(&b_id) {
                        for &pos in positions {
                            bs.set(pos as usize);
                        }
                        block_total_idf += term_idfs[term] * expanded_terms[term];
                    }
                }
                term_bitsets.push(bs);
            }

            let active_bitsets: Vec<&BitSet> = term_bitsets.iter().filter(|b| !b.is_empty()).collect();
            if active_bitsets.is_empty() { continue; }

            let mut proximity_score = 1.0;
            let mut density_mask = active_bitsets[0].clone();
            density_mask.proximity_expand(PROXIMITY_DILATION);
            
            for i in 1..active_bitsets.len() {
                let mut intersection = active_bitsets[i].clone();
                for (w_res, w_mask) in intersection.words.iter_mut().zip(density_mask.words.iter()) {
                    *w_res &= *w_mask;
                }
                
                if !intersection.is_empty() {
                    proximity_score += 1.0;
                } else {
                    proximity_score += PROXIMITY_PENALTY;
                }
                
                let mut next_dilation = active_bitsets[i].clone();
                next_dilation.proximity_expand(PROXIMITY_DILATION);
                for (w_res, w_mask) in density_mask.words.iter_mut().zip(next_dilation.words.iter()) {
                    *w_res |= *w_mask;
                }
            }
            
            block_scores.push((b_id, block_total_idf * proximity_score));
        }

        block_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let top_blocks: Vec<u32> = block_scores.into_iter().take(20).map(|(id, _)| id).collect();

        // Group reads by file descriptor to prevent excessive disk seeks
        let mut file_cache: HashMap<u32, File> = HashMap::new();
        let mut results = Vec::new();

        for b_id in top_blocks {
            let (f_id, offset, len) = self.blocks[b_id as usize];
            let path = &self.files[f_id as usize];

            let f = file_cache.entry(f_id).or_insert_with(|| File::open(path).unwrap());
            if f.seek(SeekFrom::Start(offset)).is_ok() {
                let mut buffer = vec![0; len as usize];
                if f.read_exact(&mut buffer).is_ok() {
                    let text = String::from_utf8_lossy(&buffer);
                    let text_lower = text.to_lowercase();
                    
                    let mut snippet_pos = None;
                    for term in expanded_terms.keys() {
                        if let Some(idx) = find_substring(&text_lower, term) {
                            snippet_pos = Some(idx);
                            break;
                        }
                    }

                    if let Some(idx) = snippet_pos {
                        let mut start = idx.saturating_sub(100);
                        let mut end = std::cmp::min(text.len(), idx + 200);

                        while start > 0 && !text.is_char_boundary(start) {
                            start -= 1;
                        }
                        while end < text.len() && !text.is_char_boundary(end) {
                            end += 1;
                        }

                        results.push(SearchResult {
                            file_path: path.clone(),
                            snippet: format!("...{}...", &text[start..end]),
                        });
                    }
                }
            }
        }
        results
    }

    pub fn print_status(&self) {
        println!("Index Status:");
        println!("  Files indexed: {}", self.files.len());
        println!("  Total blocks:  {}", self.blocks.len());
        println!("  Unique terms:  {}", self.inverted_index.len());
        println!("Files:");
        for (i, f) in self.files.iter().enumerate() {
            println!("  [{}] {}", i, f);
        }
    }
}
