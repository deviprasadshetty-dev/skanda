#[derive(Clone, Debug)]
pub struct BitSet {
    pub words: Vec<u64>,
}

impl BitSet {
    pub fn new(size_bits: usize) -> Self {
        let num_words = (size_bits + 63) / 64;
        Self { words: vec![0; num_words] }
    }

    pub fn set(&mut self, bit: usize) {
        if let Some(word) = self.words.get_mut(bit / 64) {
            *word |= 1 << (bit % 64);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Expands the bitset by distance `d` in both directions.
    pub fn proximity_expand(&mut self, d: usize) {
        if d == 0 { return; }
        let original = self.clone();
        
        // Expand Right (Forward in text)
        self.expand_direction(d, true);
        
        // Expand Left (Backward in text)
        let mut left_expanded = original;
        left_expanded.expand_direction(d, false);
        
        for (a, b) in self.words.iter_mut().zip(left_expanded.words.iter()) {
            *a |= *b;
        }
    }

    fn expand_direction(&mut self, d: usize, forward: bool) {
        let mut current_dist = 1;
        while current_dist <= d {
            if forward {
                self.or_shl_self(current_dist);
            } else {
                self.or_shr_self(current_dist);
            }
            // Logarithmic expansion: 1, 2, 4, 8...
            if let Some(next_dist) = current_dist.checked_mul(2) {
                if next_dist <= d {
                    current_dist = next_dist;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        // Handle remaining distance
        let remaining = d - current_dist;
        if remaining > 0 {
            if forward {
                self.or_shl_self(remaining);
            } else {
                self.or_shr_self(remaining);
            }
        }
    }

    fn or_shl_self(&mut self, k: usize) {
        let q = k / 64;
        let r = k % 64;
        let n = self.words.len();
        if q >= n {
            for w in self.words.iter_mut() { *w = 0; }
            return;
        }

        if r == 0 {
            for i in (q..n).rev() {
                self.words[i] |= self.words[i - q];
            }
        } else {
            let r_inv = 64 - r;
            for i in (q + 1..n).rev() {
                let shifted = (self.words[i - q] << r) | (self.words[i - q - 1] >> r_inv);
                self.words[i] |= shifted;
            }
            self.words[q] |= self.words[0] << r;
        }
    }

    fn or_shr_self(&mut self, k: usize) {
        let q = k / 64;
        let r = k % 64;
        let n = self.words.len();
        if q >= n {
            for w in self.words.iter_mut() { *w = 0; }
            return;
        }

        if r == 0 {
            for i in 0..n - q {
                self.words[i] |= self.words[i + q];
            }
        } else {
            let r_inv = 64 - r;
            for i in 0..n - q - 1 {
                let shifted = (self.words[i + q] >> r) | (self.words[i + q + 1] << r_inv);
                self.words[i] |= shifted;
            }
            let last_idx = n - q - 1;
            self.words[last_idx] |= self.words[n - 1] >> r;
        }
    }
}
