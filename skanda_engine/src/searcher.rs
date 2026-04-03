use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use memmap2::Mmap;
use crate::simd_search::find_substring;
use crate::compression::decode_inverted_entry;
use crate::bktree::BKTree;
use crate::indexer::{INDEX_MAGIC, INDEX_VERSION};
use crate::SkandaError;

// BM25 tuning constants
const K1: f32 = 1.2;
const B: f32 = 0.75;
// Proximity: score decays as terms spread apart (in bytes within the block)
const PROXIMITY_DECAY: f32 = 50.0;
// Maximum results returned
const TOP_K: usize = 20;
// Minimum offset gap (bytes) between two results from the same file to keep both
const DEDUP_WINDOW: u64 = 500;
// Query result cache capacity before clearing
const CACHE_CAP: usize = 256;
// Minimum term length to include in BK-tree (short terms produce noisy fuzzy matches)
const MIN_FUZZY_TERM_LEN: usize = 3;

const MAX_LOAD_CAPACITY: usize = 1_000_000;

pub struct Searcher {
    inverted_index: HashMap<String, HashMap<u32, Vec<u32>>>,
    // (file_id, byte_offset, byte_length, term_count)
    blocks: Vec<(u32, u64, u32, u16)>,
    files: Vec<String>,
    avg_block_terms: f32,
    bk_tree: BKTree,
    query_cache: Mutex<HashMap<String, Vec<SearchResult>>>,
}

#[derive(serde::Serialize, Clone)]
pub struct SearchResult {
    pub file_path: String,
    pub snippet: String,
}

// ---------------------------------------------------------------------------
// Query parsing
// ---------------------------------------------------------------------------

/// Splits a query into individual terms and quoted phrases.
/// Stop words are filtered from regular terms but kept inside phrases
/// (phrase terms that don't exist in the index simply won't match).
fn parse_query(query: &str) -> (Vec<String>, Vec<Vec<String>>) {
    use crate::indexer::is_stop_word;

    let mut regular_terms: Vec<String> = Vec::new();
    let mut phrases: Vec<Vec<String>> = Vec::new();

    let mut in_quote = false;
    let mut current_phrase_terms: Vec<String> = Vec::new();
    let mut current_token = String::new();

    for ch in query.chars() {
        if ch == '"' {
            if in_quote {
                if !current_token.is_empty() {
                    current_phrase_terms.push(current_token.to_lowercase());
                    current_token.clear();
                }
                if current_phrase_terms.len() >= 2 {
                    phrases.push(current_phrase_terms.clone());
                } else {
                    regular_terms.extend(current_phrase_terms.drain(..));
                }
                current_phrase_terms.clear();
                in_quote = false;
            } else {
                if !current_token.is_empty() {
                    let tok = current_token.to_lowercase();
                    if !is_stop_word(&tok) { regular_terms.push(tok); }
                    current_token.clear();
                }
                in_quote = true;
            }
        } else if ch.is_alphanumeric() {
            current_token.push(ch.to_lowercase().next().unwrap());
        } else {
            if !current_token.is_empty() {
                let tok = current_token.to_lowercase();
                if in_quote {
                    current_phrase_terms.push(tok);
                } else if !is_stop_word(&tok) {
                    regular_terms.push(tok);
                }
                current_token.clear();
            }
        }
    }

    // Flush remaining token
    if !current_token.is_empty() {
        let tok = current_token.to_lowercase();
        if in_quote {
            current_phrase_terms.push(tok);
        } else if !is_stop_word(&tok) {
            regular_terms.push(tok);
        }
    }
    // Unclosed quote → treat as regular terms
    if !current_phrase_terms.is_empty() {
        for t in current_phrase_terms {
            if !is_stop_word(&t) { regular_terms.push(t); }
        }
    }

    (regular_terms, phrases)
}

// ---------------------------------------------------------------------------
// Proximity scoring
// ---------------------------------------------------------------------------

/// Merge-scan two sorted position arrays to find the minimum distance.
fn min_position_distance(a: &[u32], b: &[u32]) -> u32 {
    if a.is_empty() || b.is_empty() { return u32::MAX; }
    let mut min_dist = u32::MAX;
    let mut i = 0;
    let mut j = 0;
    while i < a.len() && j < b.len() {
        let dist = a[i].abs_diff(b[j]);
        if dist < min_dist { min_dist = dist; }
        if a[i] <= b[j] { i += 1; } else { j += 1; }
    }
    min_dist
}

/// Score based on how close query terms are to each other inside the block.
/// Returns a multiplier >= 1.0; terms right next to each other → highest bonus.
fn proximity_multiplier(term_positions: &[&Vec<u32>]) -> f32 {
    if term_positions.len() <= 1 { return 1.0; }
    let mut bonus = 0.0f32;
    let n = term_positions.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let dist = min_position_distance(term_positions[i], term_positions[j]);
            if dist != u32::MAX {
                bonus += 1.0 / (1.0 + dist as f32 / PROXIMITY_DECAY);
            }
        }
    }
    1.0 + bonus
}

// ---------------------------------------------------------------------------
// Phrase matching
// ---------------------------------------------------------------------------

/// Returns true if the phrase terms appear consecutively inside the block.
/// Matches are tolerant of ±2 bytes to account for multi-byte separators.
fn phrase_in_block(phrase: &[String], positions: &HashMap<&String, &Vec<u32>>) -> bool {
    let first_positions = match positions.get(&phrase[0]) {
        Some(p) => p,
        None => return false,
    };

    'outer: for &start_pos in *first_positions {
        let mut expected = start_pos + phrase[0].len() as u32 + 1;
        for term in &phrase[1..] {
            match positions.get(term) {
                Some(pos_list) => {
                    if !pos_list.iter().any(|&p| p.abs_diff(expected) <= 2) {
                        continue 'outer;
                    }
                    expected += term.len() as u32 + 1;
                }
                None => continue 'outer,
            }
        }
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Snippet deduplication
// ---------------------------------------------------------------------------

/// From a score-sorted block list, keep only the best block per file "passage".
/// Two blocks are considered the same passage if they overlap or are within
/// DEDUP_WINDOW bytes of each other in the same file.
fn deduplicate(scored: &[(u32, f32)], blocks: &[(u32, u64, u32, u16)]) -> Vec<u32> {
    let mut kept: Vec<(u32, u64, u32)> = Vec::new(); // (file_id, offset, len)
    let mut result = Vec::new();

    'next: for &(b_id, _score) in scored {
        let (f_id, offset, len, _) = blocks[b_id as usize];
        let b_end = offset + len as u64;

        for &(kf, ko, kl) in &kept {
            if kf != f_id { continue; }
            let k_end = ko + kl as u64;
            let gap = if offset >= k_end { offset - k_end }
                      else if ko >= b_end { ko - b_end }
                      else { 0 }; // overlapping
            if gap < DEDUP_WINDOW {
                continue 'next;
            }
        }

        kept.push((f_id, offset, len));
        result.push(b_id);
        if result.len() >= TOP_K { break; }
    }
    result
}

// ---------------------------------------------------------------------------
// Searcher
// ---------------------------------------------------------------------------

impl Searcher {
    pub fn load_from_disk<P: AsRef<Path>>(index_path: P) -> Result<Self, SkandaError> {
        let file = File::open(index_path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let buffer = &mmap[..];

        // --- helpers ---
        fn read_u16(buf: &[u8], c: &mut usize) -> Option<u16> {
            if *c + 2 > buf.len() { return None; }
            let v = u16::from_le_bytes(buf[*c..*c + 2].try_into().unwrap());
            *c += 2;
            Some(v)
        }
        fn read_u32(buf: &[u8], c: &mut usize) -> Option<u32> {
            if *c + 4 > buf.len() { return None; }
            let v = u32::from_le_bytes(buf[*c..*c + 4].try_into().unwrap());
            *c += 4;
            Some(v)
        }
        fn read_u64(buf: &[u8], c: &mut usize) -> Option<u64> {
            if *c + 8 > buf.len() { return None; }
            let v = u64::from_le_bytes(buf[*c..*c + 8].try_into().unwrap());
            *c += 8;
            Some(v)
        }

        // --- magic + version ---
        if buffer.len() < 5 { return Err(SkandaError::InvalidIndex); }
        if &buffer[0..4] != INDEX_MAGIC {
            return Err(SkandaError::InvalidIndex);
        }
        let version = buffer[4];
        let mut cursor = 5;
        if version != INDEX_VERSION {
            return Err(SkandaError::InvalidIndex);
        }

        // --- files ---
        let num_files = read_u32(buffer, &mut cursor).ok_or(SkandaError::InvalidIndex)?;
        let cap = (num_files as usize).min(MAX_LOAD_CAPACITY);
        let mut files = Vec::with_capacity(cap);
        for _ in 0..num_files {
            let len = read_u16(buffer, &mut cursor).ok_or(SkandaError::InvalidIndex)? as usize;
            if cursor + len > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let path_str = String::from_utf8_lossy(&buffer[cursor..cursor + len]).to_string();
            cursor += len;
            files.push(path_str);
        }

        // --- blocks: 18 bytes each (file_id:4 + offset:8 + len:4 + term_count:2) ---
        let num_blocks = read_u32(buffer, &mut cursor).ok_or(SkandaError::InvalidIndex)?;
        let cap = (num_blocks as usize).min(MAX_LOAD_CAPACITY);
        let mut blocks: Vec<(u32, u64, u32, u16)> = Vec::with_capacity(cap);
        for _ in 0..num_blocks {
            let f_id   = read_u32(buffer, &mut cursor).ok_or(SkandaError::InvalidIndex)?;
            let off    = read_u64(buffer, &mut cursor).ok_or(SkandaError::InvalidIndex)?;
            let len    = read_u32(buffer, &mut cursor).ok_or(SkandaError::InvalidIndex)?;
            let tcount = read_u16(buffer, &mut cursor).ok_or(SkandaError::InvalidIndex)?;
            blocks.push((f_id, off, len, tcount));
        }

        // avg block length in terms (for BM25 normalization)
        let avg_block_terms = if blocks.is_empty() {
            1.0
        } else {
            let total: u64 = blocks.iter().map(|b| b.3 as u64).sum();
            (total as f32 / blocks.len() as f32).max(1.0)
        };

        // --- inverted index ---
        let num_terms = read_u32(buffer, &mut cursor).ok_or(SkandaError::InvalidIndex)?;
        let cap = (num_terms as usize).min(MAX_LOAD_CAPACITY);
        let mut inverted_index: HashMap<String, HashMap<u32, Vec<u32>>> =
            HashMap::with_capacity(cap);

        for _ in 0..num_terms {
            let term_len = read_u16(buffer, &mut cursor)
                .ok_or(SkandaError::InvalidIndex)? as usize;
            if cursor + term_len > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let term = String::from_utf8_lossy(&buffer[cursor..cursor + term_len]).to_string();
            cursor += term_len;

            let encoded_len = read_u32(buffer, &mut cursor)
                .ok_or(SkandaError::InvalidIndex)? as usize;
            if cursor + encoded_len > buffer.len() { return Err(SkandaError::InvalidIndex); }
            let decoded = decode_inverted_entry(&buffer[cursor..cursor + encoded_len]);
            cursor += encoded_len;

            let mut block_map = HashMap::with_capacity(decoded.len().min(100));
            for (b_id, positions) in decoded {
                block_map.insert(b_id, positions);
            }
            inverted_index.insert(term, block_map);
        }

        // --- build BK-tree from vocabulary ---
        let mut bk_tree = BKTree::new();
        for term in inverted_index.keys() {
            if term.len() >= MIN_FUZZY_TERM_LEN {
                bk_tree.insert(term);
            }
        }

        Ok(Self {
            inverted_index,
            blocks,
            files,
            avg_block_terms,
            bk_tree,
            query_cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn search(&self, query: &str, fuzzy: bool) -> Vec<SearchResult> {
        let cache_key = format!("{}:{}", query, fuzzy as u8);
        {
            let cache = self.query_cache.lock().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                return cached.clone();
            }
        }

        let (regular_terms, phrases) = parse_query(query);

        // Collect all terms that need to be looked up (regular + all phrase terms)
        let mut all_query_terms: HashSet<String> = regular_terms.into_iter().collect();
        for phrase in &phrases {
            for t in phrase { all_query_terms.insert(t.clone()); }
        }

        if all_query_terms.is_empty() && phrases.is_empty() {
            return vec![];
        }

        // Expand with fuzzy neighbours via BK-tree (O(log |vocab|) per term)
        let mut expanded: HashMap<String, f32> = HashMap::new();
        for term in &all_query_terms {
            expanded.insert(term.clone(), 1.0);
            if fuzzy {
                for neighbour in self.bk_tree.search(term, 1) {
                    if neighbour != term {
                        expanded.entry(neighbour.to_string())
                            .and_modify(|w| *w = w.max(0.7))
                            .or_insert(0.7);
                    }
                }
            }
        }

        // BM25 IDF per term
        let n = self.blocks.len() as f32;
        let mut term_idf: HashMap<&str, f32> = HashMap::new();
        for term in expanded.keys() {
            let df = self.inverted_index.get(term.as_str())
                .map(|m| m.len())
                .unwrap_or(0) as f32;
            // Robertson-Sparck Jones IDF (always positive)
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            term_idf.insert(term, idf);
        }

        // Gather candidate blocks
        let mut candidates: HashSet<u32> = HashSet::new();
        for term in expanded.keys() {
            if let Some(block_map) = self.inverted_index.get(term.as_str()) {
                for &b_id in block_map.keys() {
                    candidates.insert(b_id);
                }
            }
        }

        // Score each candidate
        let mut block_scores: Vec<(u32, f32)> = Vec::with_capacity(candidates.len());
        for b_id in candidates {
            let (_, _, _, term_count) = self.blocks[b_id as usize];
            let dl = term_count as f32;

            let mut bm25_total = 0.0f32;
            // Positions for each term present in this block (for proximity + phrase)
            let mut present_positions: Vec<(&String, &Vec<u32>)> = Vec::new();

            for (term, &weight) in &expanded {
                if let Some(block_map) = self.inverted_index.get(term.as_str()) {
                    if let Some(positions) = block_map.get(&b_id) {
                        let tf = positions.len() as f32;
                        let idf = term_idf[term.as_str()];
                        let norm_tf = tf * (K1 + 1.0)
                            / (tf + K1 * (1.0 - B + B * dl / self.avg_block_terms));
                        bm25_total += idf * norm_tf * weight;
                        present_positions.push((term, positions));
                    }
                }
            }

            if bm25_total <= 0.0 { continue; }

            // Proximity: min-distance between each pair of terms found in this block
            let pos_refs: Vec<&Vec<u32>> = present_positions.iter().map(|(_, p)| *p).collect();
            let prox = proximity_multiplier(&pos_refs);

            // Phrase boost: confirmed consecutive positions → 2.5×
            let phrase_boost = phrases.iter().any(|phrase| {
                let pos_map: HashMap<&String, &Vec<u32>> =
                    present_positions.iter().map(|&(t, p)| (t, p)).collect();
                phrase_in_block(phrase, &pos_map)
            });
            let boost = if phrase_boost { 2.5 } else { 1.0 };

            block_scores.push((b_id, bm25_total * prox * boost));
        }

        block_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Deduplicate: drop blocks too close to a better-scored block from the same file
        let top_blocks = deduplicate(&block_scores, &self.blocks);

        // Extract snippets
        let mut file_handles: HashMap<u32, File> = HashMap::new();
        let mut results = Vec::with_capacity(top_blocks.len());

        for b_id in top_blocks {
            let (f_id, offset, len, _) = self.blocks[b_id as usize];
            let path = &self.files[f_id as usize];

            let f = file_handles.entry(f_id)
                .or_insert_with(|| File::open(path).expect("file disappeared after indexing"));
            if f.seek(SeekFrom::Start(offset)).is_err() { continue; }
            let mut buf = vec![0u8; len as usize];
            if f.read_exact(&mut buf).is_err() { continue; }

            let text = String::from_utf8_lossy(&buf);
            let text_lower = text.to_lowercase();

            // Find first hit position for the snippet window
            let mut hit = None;
            for term in expanded.keys() {
                if let Some(idx) = find_substring(&text_lower, term) {
                    hit = Some(idx);
                    break;
                }
            }

            if let Some(idx) = hit {
                let mut start = idx.saturating_sub(100);
                let mut end = (idx + 200).min(text.len());
                while start > 0 && !text.is_char_boundary(start) { start -= 1; }
                while end < text.len() && !text.is_char_boundary(end) { end += 1; }
                results.push(SearchResult {
                    file_path: path.clone(),
                    snippet: format!("...{}...", &text[start..end]),
                });
            }
        }

        // Cache result
        {
            let mut cache = self.query_cache.lock().unwrap();
            if cache.len() >= CACHE_CAP { cache.clear(); }
            cache.insert(cache_key, results.clone());
        }

        results
    }

    pub fn print_status(&self) {
        println!("Index Status:");
        println!("  Files indexed:     {}", self.files.len());
        println!("  Total blocks:      {}", self.blocks.len());
        println!("  Unique terms:      {}", self.inverted_index.len());
        println!("  Avg terms/block:   {:.1}", self.avg_block_terms);
        println!("  BK-tree nodes:     {}", self.bk_tree.len());
        println!("Files:");
        for (i, f) in self.files.iter().enumerate() {
            println!("  [{}] {}", i, f);
        }
    }
}
