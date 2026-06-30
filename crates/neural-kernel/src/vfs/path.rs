//! Path utilities for VFS.
//! Canonicalize, split, join, normalize.

use alloc::string::String;
use alloc::vec::Vec;

/// Split path into components
/// "/foo/bar/baz" → ["foo", "bar", "baz"]
pub fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| String::from(s))
        .collect()
}

/// Join components into path
/// ["foo", "bar", "baz"] → "/foo/bar/baz"
pub fn join_path(parts: &[String]) -> String {
    if parts.is_empty() { return String::from("/"); }
    let mut path = String::from("/");
    for (i, part) in parts.iter().enumerate() {
        if i > 0 { path.push('/'); }
        path.push_str(part);
    }
    path
}

/// Canonicalize path: resolve "..", ".", remove double slashes
/// "/foo/./bar/../baz//" → "/foo/baz"
pub fn canonicalize(path: &str) -> String {
    let parts = split_path(path);
    let mut result: Vec<String> = Vec::with_capacity(parts.len());

    for part in parts {
        match part.as_str() {
            "." => continue,
            ".." => { result.pop(); }
            _ => result.push(part),
        }
    }

    join_path(&result)
}

/// Extracts filename from path
/// "/foo/bar/baz.txt" → "baz.txt"
pub fn filename(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    path.rsplit('/').next().unwrap_or("")
}

/// Extracts parent directory from path
/// "/foo/bar/baz.txt" → "/foo/bar"
pub fn parent(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    let last_slash = path.rfind('/').unwrap_or(0);
    if last_slash == 0 { return "/"; }
    &path[..last_slash]
}

/// Check if path is valid (no null bytes, no empty components except root)
pub fn is_valid(path: &str) -> bool {
    if path.is_empty() { return false; }
    if path.contains('\0') { return false; }
    if !path.starts_with('/') { return false; }
    true
}

/// Check if path exists in a list of mount points
pub fn match_mount<'a>(path: &str, mounts: &[&'a str]) -> Option<&'a str> {
    let path = path.trim_end_matches('/');
    // Check exact match first, then longest prefix
    let mut best_match: Option<&str> = None;
    for &m in mounts {
        let m = m.trim_end_matches('/');
        if path == m { return Some(m); }
        if path.starts_with(m) {
            match best_match {
                Some(current) if m.len() > current.len() => best_match = Some(m),
                None => best_match = Some(m),
                _ => {},
            }
        }
    }
    best_match
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split() {
        assert_eq!(split_path("/foo/bar"), vec!["foo", "bar"]);
        assert_eq!(split_path("/"), Vec::<String>::new());
    }

    #[test]
    fn test_canonicalize() {
        assert_eq!(canonicalize("/foo/./bar/../baz"), "/foo/baz");
        assert_eq!(canonicalize("//foo///bar//"), "/foo/bar");
    }

    #[test]
    fn test_filename() {
        assert_eq!(filename("/foo/bar/baz.txt"), "baz.txt");
        assert_eq!(filename("/foo/bar/"), "bar");
    }

    #[test]
    fn test_parent() {
        assert_eq!(parent("/foo/bar/baz.txt"), "/foo/bar");
        assert_eq!(parent("/foo"), "/");
    }
}
