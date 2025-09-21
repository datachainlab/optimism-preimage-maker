use crate::transport::Transport;
use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_transport::TransportResult;
use axum::async_trait;
use axum::http::HeaderMap;
use quick_cache::sync::Cache as QuickCache;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

pub type Cache = Arc<QuickCache<String, ResponsePacket>>;

pub fn new_cache(size: usize) -> Cache {
    let cache = QuickCache::new(size);
    Arc::new(cache)
}

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
        self.requests
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn miss(&self) {
        self.misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_requests(&self) -> u64 {
        self.requests.load(std::sync::atomic::Ordering::Relaxed)
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
            let result = self
                .cache
                .get_or_insert_async(&hash, async {
                    self.metrics.miss();
                    self.inner.post(req, headers).await
                })
                .await?;
            self.metrics.request();
            return Ok(result);
        };
        self.inner.post(req, headers).await
    }
}
