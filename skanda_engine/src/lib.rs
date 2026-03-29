pub mod indexer;
pub mod searcher;
pub mod bridge;
pub mod simd_search;
pub mod bitset;
pub mod compression;
pub mod thread_pool;
pub mod fuzzy_search;

pub use indexer::Indexer;
pub use searcher::{Searcher, SearchResult};
pub use bridge::Bridge;

#[derive(Debug)]
pub enum SkandaError {
    Io(std::io::Error),
    InvalidIndex,
    EmptyQuery,
}

impl From<std::io::Error> for SkandaError {
    fn from(err: std::io::Error) -> Self {
        SkandaError::Io(err)
    }
}

impl std::fmt::Display for SkandaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkandaError::Io(e) => write!(f, "IO error: {}", e),
            SkandaError::InvalidIndex => write!(f, "Invalid or corrupted index file"),
            SkandaError::EmptyQuery => write!(f, "Search query cannot be empty"),
        }
    }
}

impl std::error::Error for SkandaError {}
