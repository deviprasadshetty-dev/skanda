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

    let mut dp = vec![vec![0; m + 1]; n + 1];

    for i in 0..=n { dp[i][0] = i; }
    for j in 0..=m { dp[0][j] = j; }

    for i in 1..=n {
        for j in 1..=m {
            let cost = if v1[i-1] == v2[j-1] { 0 } else { 1 };
            dp[i][j] = std::cmp::min(
                std::cmp::min(dp[i-1][j] + 1, dp[i][j-1] + 1),
                dp[i-1][j-1] + cost
            );
        }
    }

    dp[n][m]
}
