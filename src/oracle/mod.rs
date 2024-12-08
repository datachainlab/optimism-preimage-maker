use std::sync::Arc;
use kona_preimage::PreimageKey;
use lru::LruCache;

pub mod write_through;
pub mod lockfree;
pub mod r#unsafe;
pub mod multiprocess;

pub type Cache = Arc<spin::Mutex<LruCache<PreimageKey, Vec<u8>>>>;

pub fn new_cache() -> Cache {
    Arc::new(spin::Mutex::new(LruCache::unbounded()))
}