//! Optional near-duplicate detection (feature `similarity`).
//! Stub: wire MinHash/LSH here without affecting the exact-dedupe hot path.
//!
//! Enable with `cargo build --features similarity`.

#[cfg(feature = "similarity")]
pub fn note_enabled() -> &'static str {
    "Similarity mode is enabled (stub — implement MinHash + bands per Implementation.md §14)."
}

#[cfg(not(feature = "similarity"))]
pub fn note_enabled() -> &'static str {
    "Exact-dedupe mode (optional: cargo build --features similarity for near-duplicate stub)."
}
