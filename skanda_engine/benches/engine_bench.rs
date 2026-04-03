use criterion::{black_box, criterion_group, criterion_main, Criterion};
use skanda_engine::{Indexer, Searcher};
use std::path::Path;

fn bench_indexer(c: &mut Criterion) {
    let data_dir = Path::new("data");
    
    // Ensure data dir exists for benchmark (assumes aliceinwonderland.txt is inside)
    if data_dir.exists() {
        c.bench_function("indexer_alice_in_wonderland", |b| {
            b.iter(|| {
                let mut indexer = Indexer::new();
                indexer.index_directory(black_box(data_dir));
                let _ = indexer.save_to_disk("data/bench_index.bin");
            })
        });
    }
}

fn bench_searcher(c: &mut Criterion) {
    let index_path = "data/bench_index.bin";
    
    if Path::new(index_path).exists() {
        let searcher = Searcher::load_from_disk(index_path).expect("Failed to load index");
        
        c.bench_function("search_alice_in_wonderland_exact", |b| {
            b.iter(|| {
                searcher.search(black_box("rabbit hole"), black_box(false))
            })
        });

        c.bench_function("search_alice_in_wonderland_fuzzy", |b| {
            b.iter(|| {
                searcher.search(black_box("rabbit hole"), black_box(true))
            })
        });
    }
}

criterion_group!(benches, bench_indexer, bench_searcher);
criterion_main!(benches);