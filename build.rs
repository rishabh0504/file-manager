fn main() {
    // The `mpv` feature links system libmpv. Homebrew (and MacPorts) install dirs
    // are not on the linker's default search path, so add them explicitly.
    if std::env::var_os("CARGO_FEATURE_MPV").is_some() {
        for dir in [
            "/opt/homebrew/lib", // Apple Silicon Homebrew
            "/usr/local/lib",    // Intel Homebrew
            "/opt/local/lib",    // MacPorts
        ] {
            if std::path::Path::new(dir).exists() {
                println!("cargo:rustc-link-search=native={dir}");
            }
        }
    }
}
