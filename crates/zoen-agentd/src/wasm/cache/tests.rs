//! Real Wasmtime cache lookups, not a mock compiled-artifact store.
#![cfg(unix)]
use super::*;
use crate::{wasm::WasmTier, Needs, ToolManifest};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::os::unix::fs::{symlink, PermissionsExt};

const COMPONENT: &[u8] = b"\0asm\x0d\0\x01\0";
fn key() -> SigningKey {
    SigningKey::from_bytes(&[42; 32])
}
fn manifest(needs: Needs) -> crate::SignedManifest {
    ToolManifest {
        id: "cache-test".into(),
        version: "1".into(),
        summary: "Actual minimal component".into(),
        needs,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(hex::encode(Sha256::digest(COMPONENT))),
    }
    .sign(&key())
}
fn private() -> (tempfile::TempDir, CompilationCache) {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap().join("compiled");
    std::fs::create_dir(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cache = CompilationCache::private_directory(&directory).unwrap();
    (root, cache)
}
fn tier(cache: &CompilationCache) -> WasmTier {
    WasmTier::with_compilation_cache(vec![key().verifying_key()], 1, 256 * 1024 * 1024, cache)
        .unwrap()
}
fn entries(path: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for item in std::fs::read_dir(path).unwrap() {
        let path = item.unwrap().path();
        if path.is_dir() {
            result.extend(entries(&path));
        } else if path.extension().is_none() {
            result.push(path);
        }
    }
    result
}

#[test]
fn new_engine_hits_official_cache_and_small_truncated_entry_recompiles() {
    let (_root, cache) = private();
    let signed = manifest(Needs::Wasm);
    tier(&cache).load(&signed, COMPONENT).unwrap();
    assert_eq!(cache.stats(), CompilationCacheStats { hits: 0, misses: 1 });
    let files = entries(&cache.directory.join("modules"));
    assert_eq!(files.len(), 1);
    assert!(std::fs::metadata(&files[0]).unwrap().len() > 9);
    tier(&cache).load(&signed, COMPONENT).unwrap();
    assert_eq!(cache.stats(), CompilationCacheStats { hits: 1, misses: 1 });
    std::fs::write(&files[0], b"truncated").unwrap();
    tier(&cache).load(&signed, COMPONENT).unwrap();
    assert_eq!(cache.stats(), CompilationCacheStats { hits: 1, misses: 2 });
    assert!(std::fs::metadata(&files[0]).unwrap().len() > 9);
    tier(&cache).load(&signed, COMPONENT).unwrap();
    assert_eq!(cache.stats(), CompilationCacheStats { hits: 2, misses: 2 });
}

#[test]
fn publisher_signature_needs_and_hash_refuse_before_any_artifact_lookup() {
    let (_root, cache) = private();
    let tier = tier(&cache);
    let signed = manifest(Needs::Wasm);
    let mut forged = signed.clone();
    forged.signature.clear();
    assert!(matches!(
        tier.load(&forged, COMPONENT),
        Err(WasmError::Manifest(_))
    ));
    let untrusted = signed
        .manifest
        .clone()
        .sign(&SigningKey::from_bytes(&[19; 32]));
    assert!(matches!(
        tier.load(&untrusted, COMPONENT),
        Err(WasmError::Manifest(_))
    ));
    assert!(matches!(
        tier.load(&manifest(Needs::None), COMPONENT),
        Err(WasmError::NotWasm)
    ));
    assert!(matches!(
        tier.load(&signed, b"swapped"),
        Err(WasmError::WrongComponent)
    ));
    assert_eq!(cache.stats(), CompilationCacheStats { hits: 0, misses: 0 });
    assert!(!cache.directory.join("modules").exists());
}

#[test]
fn unsafe_paths_are_refused_instead_of_adopted_or_silently_uncached() {
    let (root, cache) = private();
    let root = root.path().canonicalize().unwrap();
    assert!(CompilationCache::private_directory(Path::new("relative")).is_err());
    assert!(CompilationCache::private_directory(&root.join("missing")).is_err());
    let file = root.join("file");
    std::fs::write(&file, b"not a directory").unwrap();
    assert!(CompilationCache::private_directory(&file).is_err());
    let link = root.join("link");
    symlink(&cache.directory, &link).unwrap();
    assert!(CompilationCache::private_directory(&link).is_err());
    std::fs::create_dir(cache.directory.join("child")).unwrap();
    std::fs::set_permissions(
        cache.directory.join("child"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    assert!(CompilationCache::private_directory(&link.join("child")).is_err());
    std::fs::set_permissions(&cache.directory, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(CompilationCache::private_directory(&cache.directory).is_err());
    std::fs::set_permissions(&cache.directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let unsafe_parent = root.join("unsafe-parent");
    std::fs::create_dir(&unsafe_parent).unwrap();
    std::fs::set_permissions(&unsafe_parent, std::fs::Permissions::from_mode(0o777)).unwrap();
    let child = unsafe_parent.join("compiled");
    std::fs::create_dir(&child).unwrap();
    std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(CompilationCache::private_directory(&child).is_err());
}

#[test]
fn permission_change_or_directory_replacement_refuses_before_lookup() {
    let (_root, cache) = private();
    let tier = tier(&cache);
    std::fs::set_permissions(&cache.directory, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        tier.load(&manifest(Needs::Wasm), COMPONENT),
        Err(WasmError::CacheConfiguration(_))
    ));
    // Even an unsafe cache does not supersede the original signature/hash gates.
    assert!(matches!(
        tier.load(&manifest(Needs::Wasm), b"swapped"),
        Err(WasmError::WrongComponent)
    ));
    std::fs::set_permissions(&cache.directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::rename(&cache.directory, cache.directory.with_extension("retired")).unwrap();
    std::fs::create_dir(&cache.directory).unwrap();
    std::fs::set_permissions(&cache.directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        tier.load(&manifest(Needs::Wasm), COMPONENT),
        Err(WasmError::CacheConfiguration(_))
    ));
    assert_eq!(cache.stats(), CompilationCacheStats { hits: 0, misses: 0 });
}

#[test]
fn original_constructor_stays_uncached_and_memory_ceiling_is_identical() {
    let (_root, cache) = private();
    let cached = tier(&cache);
    let original = WasmTier::new(vec![key().verifying_key()], 1, 256 * 1024 * 1024).unwrap();
    assert!(original.cache.is_none());
    assert_eq!(original.max_memory, cached.max_memory);
    original.load(&manifest(Needs::Wasm), COMPONENT).unwrap();
    assert_eq!(cache.stats(), CompilationCacheStats { hits: 0, misses: 0 });
}
