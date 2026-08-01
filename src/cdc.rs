//! Content-defined chunking (feature `cdc`). Stub for Rabin/CDC integration.
#[cfg(feature = "cdc")]
pub fn note_enabled() -> &'static str {
    "CDC feature enabled (stub — add cdchunking per Implementation.md)."
}

#[cfg(not(feature = "cdc"))]
pub fn note_enabled() -> &'static str {
    "Build with --features cdc for content-defined chunking experiments."
}
