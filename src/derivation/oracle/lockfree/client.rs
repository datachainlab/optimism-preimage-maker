use crate::derivation::oracle::lockfree::{HintSender, PreimageSender};
use crate::derivation::oracle::Cache;
use alloy_primitives::B256;
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use kona_preimage::{HintWriterClient, PreimageFetcher, PreimageKey, PreimageOracleClient};
use lru::LruCache;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};

#[derive(Clone, Debug)]
pub struct PreimageIO {
    cache: Arc<spin::Mutex<LruCache<PreimageKey, Vec<u8>>>>,
    hint_sender: async_channel::Sender<(String, HintSender)>,
    preimage_sender: async_channel::Sender<(B256, PreimageSender)>,
}

impl PreimageIO {
    pub fn new(
        cache: Cache,
        hint_sender: async_channel::Sender<(String, HintSender)>,
        preimage_sender: async_channel::Sender<(B256, PreimageSender)>,
    ) -> Self {
        Self {
            cache,
            hint_sender,
            preimage_sender,
        }
    }
}

#[async_trait::async_trait]
impl PreimageOracleClient for PreimageIO {
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let mut cache_lock = self.cache.lock();
        if let Some(value) = cache_lock.get(&key) {
            return Ok(value.clone());
        }

        let (sender, mut receiver) = oneshot::channel();

        self.preimage_sender
            .send((key.into(), sender))
            .await
            .map_err(|e| PreimageOracleError::Other(e.to_string()))?;

        let result = receiver
            .await
            .map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        let result = result.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        cache_lock.put(key, result.clone());
        Ok(result)
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        let data = self.get(key).await?;
        buf.copy_from_slice(data.as_slice());
        Ok(())
    }
}

#[async_trait::async_trait]
impl HintWriterClient for PreimageIO {
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        let (sender, mut receiver) = oneshot::channel();

        self.hint_sender
            .send((hint.to_string(), sender))
            .await
            .map_err(|e| PreimageOracleError::Other(e.to_string()))?;

        _ = receiver
            .await
            .map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        Ok(())
    }
}
