use glob::Pattern;
use std::path::Path;

pub fn compile_patterns(raw: &[String]) -> Vec<Pattern> {
    raw.iter()
        .filter_map(|s| Pattern::new(s).ok())
        .collect()
}

pub fn path_matches_any(path: &Path, patterns: &[Pattern]) -> bool {
    let s = path.to_string_lossy();
    patterns.iter().any(|p| p.matches_path(Path::new(s.as_ref())))
}
