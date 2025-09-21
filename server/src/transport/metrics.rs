use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;

#[derive(Debug)]
pub struct Metrics {
    requests: AtomicU64,
    misses: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            requests: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }
    pub fn request(&self) {
        self.requests.fetch_add(1, Relaxed);
    }

    pub fn miss(&self) {
        self.misses.fetch_add(1, Relaxed);
    }

    pub fn get_requests(&self) -> u64 {
        self.requests.load(Relaxed)
    }

    pub fn get_misses(&self) -> u64 {
        self.misses.load(Relaxed)
    }
}
