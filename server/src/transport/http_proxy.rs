use crate::transport::metrics::Metrics;
use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_primitives::{keccak256, B256};
use alloy_transport::{TransportError, TransportFut, TransportResult};
use alloy_transport_http::Http;
use quick_cache::sync::Cache as QuickCache;
use reqwest::Client;
use std::sync::Arc;
use std::task;
use tower::Service;

pub type Cache = Arc<QuickCache<B256, ResponsePacket>>;

pub fn new_cache(size: usize) -> Cache {
    let cache = QuickCache::new(size);
    Arc::new(cache)
}

#[derive(Clone, Debug)]
pub struct HttpProxy {
    cache: Cache,
    metrics: Arc<Metrics>,
    inner: Http<Client>,
}

impl HttpProxy {
    pub fn new(cache: Cache, metrics: Arc<Metrics>, inner: Http<Client>) -> Self {
        Self {
            cache,
            metrics,
            inner,
        }
    }

    async fn post(mut self, req: RequestPacket) -> TransportResult<ResponsePacket> {
        let hash = match make_hash(&req) {
            Ok(hash) => hash,
            Err(err) => {
                return Err(TransportError::deser_err(
                    err,
                    "invalid request for hashing",
                ))
            }
        };

        self.metrics.request();
        let result = self
            .cache
            .get_or_insert_async(&hash, async {
                self.metrics.miss();
                self.inner.call(req).await
            })
            .await?;

        Ok(result)
    }
}

impl Service<RequestPacket> for HttpProxy {
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let this = self.clone();
        Box::pin(this.post(req))
    }
}

fn make_hash(req: &RequestPacket) -> Result<B256, serde_json::Error> {
    let body = match &req {
        RequestPacket::Single(r) => serde_json::to_vec(r)?,
        RequestPacket::Batch(_) => {
            let mut value = vec![];
            if let Some(batch) = req.as_batch() {
                for r in batch {
                    value.extend_from_slice(&r.params_hash().0);
                }
            }
            value
        }
    };
    Ok(keccak256(&body))
}
