use std::num::NonZeroUsize;
use kona_preimage::PreimageKey;
use lru::LruCache;
use std::sync::Arc;

pub mod lockfree;
pub mod multiprocess;
pub mod r#unsafe;
pub mod write_through;

pub type Cache = Arc<spin::Mutex<LruCache<PreimageKey, Vec<u8>>>>;

pub fn new_cache() -> Cache {
    Arc::new(spin::Mutex::new(LruCache::new(NonZeroUsize::new(100_000_000).unwrap())))
}
