pub mod indexer;
pub mod searcher;
pub mod bridge;
pub mod simd_search;
pub mod bitset;
pub mod compression;
pub mod thread_pool;

pub use indexer::Indexer;
pub use searcher::{Searcher, SearchResult};
pub use bridge::Bridge;

#[derive(Debug)]
pub enum PfrError {
    Io(std::io::Error),
    InvalidIndex,
    EmptyQuery,
}

impl From<std::io::Error> for PfrError {
    fn from(err: std::io::Error) -> Self {
        PfrError::Io(err)
    }
}

impl std::fmt::Display for PfrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PfrError::Io(e) => write!(f, "IO error: {}", e),
            PfrError::InvalidIndex => write!(f, "Invalid or corrupted index file"),
            PfrError::EmptyQuery => write!(f, "Search query cannot be empty"),
        }
    }
}

impl std::error::Error for PfrError {}
