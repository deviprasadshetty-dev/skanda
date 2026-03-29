pub struct FuzzyMatcher {
    pattern_len: usize,
    char_masks: [u64; 256],
    max_errors: usize,
}

impl FuzzyMatcher {
    pub fn new(pattern: &str, max_errors: usize) -> Self {
        let mut char_masks = [!0u64; 256];
        let p_bytes = pattern.as_bytes();
        let len = std::cmp::min(p_bytes.len(), 64);

        for i in 0..len {
            char_masks[p_bytes[i] as usize] &= !(1 << i);
        }

        Self {
            pattern_len: len,
            char_masks,
            max_errors,
        }
    }

    /// Finds the end position of the first fuzzy match.
    /// Returns None if no match is found within max_errors.
    pub fn find(&self, haystack: &str) -> Option<usize> {
        if self.pattern_len == 0 { return Some(0); }
        let h_bytes = haystack.as_bytes();
        
        // R[i] holds the state for i errors.
        // Bit j is 0 if the pattern prefix of length j+1 matches with i errors.
        let mut r = vec![!0u64; self.max_errors + 1];
        let match_mask = 1 << (self.pattern_len - 1);

        for (i, &byte) in h_bytes.iter().enumerate() {
            let char_mask = self.char_masks[byte as usize];
            
            let mut prev_r_error = r[0];
            // Exact match state update
            r[0] = (r[0] << 1) | char_mask;

            if r[0] & match_mask == 0 {
                return Some(i);
            }

            // Update states for 1..max_errors
            for k in 1..=self.max_errors {
                let old_r_k = r[k];
                
                // Substitution: (prev_r_error << 1)
                // Insertion: (r[k-1] << 1)
                // Deletion: prev_r_error
                // Match: (old_r_k << 1) | char_mask
                let current_match = (old_r_k << 1) | char_mask;
                let substitution = prev_r_error << 1;
                let insertion = r[k-1] << 1;
                let deletion = prev_r_error;
                
                r[k] = current_match & substitution & insertion & deletion;
                
                prev_r_error = old_r_k;

                if r[k] & match_mask == 0 {
                    return Some(i);
                }
            }
        }

        None
    }
}
/// A simplified Levenshtein helper for short strings
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let v1: Vec<char> = s1.chars().collect();
    let v2: Vec<char> = s2.chars().collect();
    let n = v1.len();
    let m = v2.len();
    
    if n == 0 { return m; }
    if m == 0 { return n; }

    let mut v0: Vec<usize> = (0..=m).collect();
    let mut v1_row = vec![0; m + 1];

    for i in 0..n {
        v1_row[0] = i + 1;
        for j in 0..m {
            let cost = if v1[i] == v2[j] { 0 } else { 1 };
            v1_row[j + 1] = std::cmp::min(
                std::cmp::min(v1_row[j] + 1, v0[j + 1] + 1),
                v0[j] + cost
            );
        }
        std::mem::swap(&mut v0, &mut v1_row);
    }

    v0[m]
}
