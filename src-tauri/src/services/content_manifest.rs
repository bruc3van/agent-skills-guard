//! Bounded, streaming content identities for installed versions and stale scan detection.
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

pub fn manifest(root: &Path) -> Result<BTreeMap<String, String>> {
    let root = root.canonicalize().context("CONTENT_ROOT_UNAVAILABLE")?;
    let mut result = BTreeMap::new();
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(&root).follow_links(false) {
        let entry = entry?;
        if entry.depth() > 64 || result.len() >= 20_000 {
            anyhow::bail!("CONTENT_INVENTORY_LIMIT");
        }
        if entry.file_type().is_dir() {
            continue;
        }
        let key = entry
            .path()
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        if entry.file_type().is_symlink() {
            result.insert(
                key,
                format!("link:{}", std::fs::read_link(entry.path())?.display()),
            );
            continue;
        }
        if !entry.file_type().is_file() {
            anyhow::bail!("CONTENT_SPECIAL_FILE");
        }
        let mut file = File::open(entry.path())?;
        let mut hash = Sha256::new();
        let mut buf = [0; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > 500 * 1024 * 1024 {
                anyhow::bail!("CONTENT_BYTE_LIMIT");
            }
            hash.update(&buf[..n]);
        }
        result.insert(key, hex::encode(hash.finalize()));
    }
    Ok(result)
}

pub fn changes(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Vec<String> {
    let mut changes = Vec::new();
    for (path, hash) in before {
        match after.get(path) {
            None => changes.push(format!("Deleted: {path}")),
            Some(new) if new != hash => changes.push(format!("Modified: {path}")),
            _ => {}
        }
    }
    for path in after.keys().filter(|p| !before.contains_key(*p)) {
        changes.push(format!("Added: {path}"));
    }
    changes
}
