use crate::transport::Transport;
use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_transport::TransportResult;
use axum::async_trait;
use axum::http::HeaderMap;
use lru::LruCache;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type Cache = Arc<RwLock<LruCache<String, ResponsePacket>>>;

pub fn new_cache(size: usize) -> Cache {
    let cache = LruCache::new(std::num::NonZeroUsize::new(size).expect("N must be greater than 0"));
    Arc::new(RwLock::new(cache))
}

#[derive(Debug)]
pub struct Metrics {
    hits: AtomicU64,
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
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }
    pub fn hit(&self) {
        self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn miss(&self) {
        self.misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_hits(&self) -> u64 {
        self.hits.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn get_misses(&self) -> u64 {
        self.misses.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Clone, Debug)]
pub struct LruProxy<T: Transport> {
    cache: Cache,
    metrics: Arc<Metrics>,
    inner: T,
}

impl<T: Transport> LruProxy<T> {
    pub fn new(cache: Cache, metrics: Arc<Metrics>, inner: T) -> Self {
        Self {
            cache,
            metrics,
            inner,
        }
    }
}

#[async_trait]
impl<T: Transport> Transport for LruProxy<T> {
    async fn post(self, req: RequestPacket, headers: HeaderMap) -> TransportResult<ResponsePacket> {
        if let Some(value) = headers.get("X-Request-Hash") {
            let hash = value.to_str().unwrap().to_string();
            let mut cache = self.cache.write().await;
            return match cache.get(&hash) {
                Some(v) => {
                    self.metrics.hit();
                    Ok(v.clone())
                }
                None => {
                    self.metrics.miss();
                    let result = self.inner.post(req, headers).await?;
                    cache.put(hash, result.clone());
                    Ok(result)
                }
            };
        };

        self.inner.post(req, headers).await
    }
}
