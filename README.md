# Predictive Footprint Retrieval (PFR) Engine

A zero-dependency, ultra-high-performance retrieval engine designed for the next generation of RAG.

## The Paradigm: Predictive Footprint Retrieval (PFR)

Current RAG systems rely on **Vector Embeddings**, which are computationally expensive, lose exact syntactic detail, and struggle with massive scale (billions of lines).

**PFR** flips the model:
1. **Predict:** Instead of embedding the corpus, we use the LLM's internal knowledge to predict the exact syntactic **"footprints"** (rare words, code patterns, or specific phrases) that would appear near the answer.
2. **Retrieve:** Our Rust engine performs massively parallel, SIMD-accelerated exact search across billions of lines in milliseconds.
3. **Rank:** Results are ranked using **IDF-weighted Positional Proximity** (BitSet-based dilation) to ensure the LLM gets the densest clusters of relevant information.

## Features
- **0 Heavy Dependencies:** Pure Rust standard library. No `Tokio`, no `Serde`, no `Rayon`.
- **Massive Scale:** Efficient Varint + Delta encoding keeps indices small and in-memory.
- **Blistering Speed:** SIMD-accelerated (AVX2/SSE2) exact matching.
- **Accuracy:** IDF-weighted proximity scoring finds the most contextually relevant snippets.

## Quickstart

### 1. Build
```bash
cargo build --release
```

### 2. Index
```bash
./pfr index <directory_to_index> index.bin
```

### 3. Search
```bash
./pfr search index.bin "your predicted footprints"
```

### 4. JSON API (Bridge)
```bash
./pfr serve index.bin 8080
```
Query via: `http://localhost:8080/search?q=footprint1+footprint2`

## LLM Integration

You can give your LLM access to PFR by providing this tool definition:

```json
{
  "name": "pfr_search",
  "description": "Search billions of lines of text in milliseconds by predicting syntactic footprints (rare words or patterns).",
  "parameters": {
    "query": "Space-separated rare words or specific code/text patterns likely to appear near the answer."
  }
}
```

## License
MIT
