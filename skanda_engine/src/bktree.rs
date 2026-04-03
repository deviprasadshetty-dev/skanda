use crate::fuzzy_search::levenshtein_distance;

struct BKNode {
    word: String,
    children: Vec<(usize, usize)>, // (distance, node_index)
}

/// BK-tree for O(log N) fuzzy vocabulary search.
/// Replaces the O(|vocabulary|) linear scan in fuzzy mode.
pub struct BKTree {
    nodes: Vec<BKNode>,
}

impl BKTree {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn insert(&mut self, word: &str) {
        if self.nodes.is_empty() {
            self.nodes.push(BKNode { word: word.to_string(), children: Vec::new() });
            return;
        }
        let mut idx = 0;
        loop {
            let dist = levenshtein_distance(word, &self.nodes[idx].word);
            if dist == 0 { return; } // duplicate

            let found_child = self.nodes[idx].children.iter()
                .find(|&&(d, _)| d == dist)
                .map(|&(_, ci)| ci);

            match found_child {
                Some(child_idx) => idx = child_idx,
                None => {
                    let new_idx = self.nodes.len();
                    self.nodes[idx].children.push((dist, new_idx));
                    self.nodes.push(BKNode { word: word.to_string(), children: Vec::new() });
                    return;
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns all words within `max_dist` edits of `query`.
    pub fn search<'a>(&'a self, query: &str, max_dist: usize) -> Vec<&'a str> {
        if self.nodes.is_empty() { return vec![]; }
        let mut results = Vec::new();
        let mut stack = vec![0usize];
        while let Some(idx) = stack.pop() {
            let node = &self.nodes[idx];
            let dist = levenshtein_distance(query, &node.word);
            if dist <= max_dist {
                results.push(node.word.as_str());
            }
            let low = dist.saturating_sub(max_dist);
            let high = dist + max_dist;
            for &(child_dist, child_idx) in &node.children {
                if child_dist >= low && child_dist <= high {
                    stack.push(child_idx);
                }
            }
        }
        results
    }
}
