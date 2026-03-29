<div align="center">
  <h1>⚡ Skanda Engine</h1>
  <p><b>Zero-dependency, ultra-high-performance retrieval engine designed for the next generation of RAG.</b></p>
</div>

---

## 🚀 The Paradigm Shift

Current Retrieval-Augmented Generation (RAG) systems heavily rely on **Vector Embeddings**. While useful, embeddings are computationally expensive to generate, lose exact syntactic detail during translation, and struggle with massive scale (e.g., billions of lines of code or text). 

**Skanda** flips the model on its head by leveraging the LLM's own internal knowledge before retrieval.

1. **🧠 Predict:** Instead of blindly embedding your entire corpus, we ask the LLM to predict the exact syntactic **"footprints"** (rare words, code patterns, specific function names, or phrases) that would logically appear near the answer.
2. **⚡ Retrieve:** Skanda’s Rust-based engine takes these footprints and performs massively parallel, SIMD-accelerated exact searches across billions of lines in mere milliseconds.
3. **🎯 Rank:** Results are dynamically ranked using **IDF-weighted Positional Proximity** (via BitSet-based dilation) to ensure the LLM receives the densest, most contextually rich clusters of relevant information.

---

## ✨ Key Features

- **Minimal Dependencies:** Built heavily on the Rust standard library, using highly targeted crates like `Rayon` for multi-threading and `Serde` for API responses. No `Tokio` runtime overhead. Just raw performance.
- **Massive Scale:** Employs efficient Varint + Delta encoding to keep indices remarkably small and fully in-memory.
- **Blistering Speed:** Leverages SIMD-accelerated (AVX2/SSE2) exact matching for near-instantaneous retrieval.
- **Hyper Accuracy:** Uses advanced IDF-weighted proximity scoring to discover the most contextually relevant snippets across your data.

---

## 🛠️ Quickstart Guide

### 1. Build the Engine
Compile the binary for maximum performance:
```bash
cargo build --release
```

### 2. Index Your Data
Create a highly compressed index of your target directory:
```bash
./skanda index <directory_to_index> index.bin
```

### 3. Search (with Typo Tolerance)
Execute a rapid search for your predicted footprints.
```bash
./skanda search index.bin "your predicted footprints" --fuzzy
```
*Note: The `--fuzzy` flag enables **Levenshtein-tolerant expansion**. If your LLM predicts `token_type_embeddings` but the actual corpus uses `token_type_ids`, Skanda's fuzzy matching will still locate the correct context.*

### 4. Deploy the JSON API (Bridge)
Expose the Skanda engine as a lightweight HTTP microservice:
```bash
./skanda serve index.bin 8080
```
Query the API directly:
```bash
curl "http://localhost:8080/search?q=footprint1+footprint2&fuzzy=true"
```

---

## 🤖 LLM Integration

Supercharge your agents by giving your LLM direct access to Skanda. Simply provide the following tool definition:

```json
{
  "name": "skanda_search",
  "description": "Search billions of lines of text in milliseconds by predicting syntactic footprints (rare words or patterns).",
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Space-separated rare words or specific code/text patterns likely to appear near the answer."
      }
    },
    "required": ["query"]
  }
}
```

---

## 🚀 Benchmarks

We constantly monitor performance regressions using `criterion`. Based on testing with **Alice's Adventures in Wonderland** (1 full novel text file), Skanda Engine remains blazing fast even after adding robust safeguards:

| Operation | Query/Task | Time per Iteration |
| :--- | :--- | :--- |
| **Indexing** | Full File parsing, chunking, and writing to disk | **~8.39 ms** (milliseconds) |
| **Exact Search** | Query: `"rabbit hole"` | **~65.25 µs** (microseconds) |
| **Fuzzy Search** | Query: `"rabbit hole"`, fuzzy match enabled | **~214.98 µs** (microseconds) |

*These results establish a very solid baseline showing zero significant performance degradation after fixing the memory-mapped loading and SIMD behaviors.*

---

## 📜 License

This project is licensed under the MIT License.
