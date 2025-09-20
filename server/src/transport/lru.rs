use std::sync::{Arc};
use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_transport::{TransportResult};
use axum::async_trait;
use axum::http::HeaderMap;
use lru::LruCache;
use tokio::sync::RwLock;
use crate::transport::Transport;

#[derive(Clone, Debug)]
struct Metrics {
    hits: u128,
    misses: u128,
}

impl Metrics {
    fn new() -> Self {
        Self { hits: 0, misses: 0 }
    }
    fn hit(&mut self) {
        self.hits += 1;
    }

    fn miss(&mut self) {
        self.misses += 1;
    }

    fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            self.hits as f64 / (self.hits + self.misses) as f64
        }
    }

    fn miss_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            self.misses as f64 / (self.hits + self.misses) as f64
        }
    }

    fn total_requests(&self) -> u128 {
        self.hits + self.misses
    }

}

#[derive(Clone, Debug)]
pub struct LruProxy<T: Transport> {
    cache: Arc<RwLock<(LruCache<String, ResponsePacket>, Metrics)>>,
    inner: T
}

impl <T:Transport> LruProxy<T> {
    pub fn new(cache_size: usize, inner: T) -> Self {
        let cache = LruCache::new(
            std::num::NonZeroUsize::new(cache_size).expect("N must be greater than 0"),
        );
        Self {
            cache: Arc::new(RwLock::new((cache,  Metrics::new()))),
            inner,
        }
    }
}

#[async_trait]
impl <T: Transport> Transport for LruProxy<T> {
    async fn post(self, req: RequestPacket, headers: HeaderMap) -> TransportResult<ResponsePacket> {
        if let Some(value) = headers.get("X-Request-Hash") {
            let hash = value.to_str().unwrap().to_string();
            let mut cache = self.cache.write().await;
            return match cache.0.get(&hash) {
                Some(v) => {
                    cache.1.hit();
                    Ok(v.clone())
                },
                None => {
                    cache.1.miss();
                    let result = self.inner.post(req, headers).await?;
                    cache.0.put(hash, result.clone());
                    Ok(result)
                }
            }
        };

        self.inner.post(req, headers).await
    }
}
