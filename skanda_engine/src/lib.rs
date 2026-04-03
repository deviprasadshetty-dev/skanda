pub mod indexer;
pub mod searcher;
pub mod bridge;
pub mod simd_search;
pub mod bitset;
pub mod compression;
pub mod fuzzy_search;
pub mod bktree;

pub use indexer::Indexer;
pub use searcher::{Searcher, SearchResult};
pub use bridge::Bridge;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SkandaError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid Index")]
    InvalidIndex,
    #[error("Empty Query")]
    EmptyQuery,
}
