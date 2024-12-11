use std::fmt::Debug;
use std::sync::Arc;
use anyhow::anyhow;
use kona_host::kv::KeyValueStore;
use kona_preimage::{HintWriterClient, PreimageFetcher, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use tokio::sync::{oneshot, RwLock};
use crate::derivation::oracle::write_through::HintSender;

pub struct PreimageIO<KV: KeyValueStore + ?Sized + Send + Sync > {
    kv_store: Arc<RwLock<KV>>,
    hint_sender: async_channel::Sender<(String, HintSender)>,
}

impl  <KV: KeyValueStore + ?Sized + Send + Sync > Debug for PreimageIO<KV> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreimageIO")
            .finish()
    }
}

impl <KV> Clone for PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync {
    fn clone(&self) -> Self {
        Self {
            kv_store: self.kv_store.clone(),
            hint_sender: self.hint_sender.clone(),
        }
    }
}


impl <KV: KeyValueStore + ?Sized + Send + Sync > PreimageIO<KV> {
    pub fn new(
        hint_sender: async_channel::Sender<(String, HintSender)>,
        kv_store: Arc<RwLock<KV>>,
    ) -> Self {
        Self {
            kv_store,
            hint_sender,
        }
    }
}

#[async_trait::async_trait]
impl <KV: KeyValueStore + ?Sized + Send + Sync > PreimageOracleClient for PreimageIO<KV> {
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let kv_lock = self.kv_store.read().await;
        kv_lock.get(key.into()).ok_or_else(|| PreimageOracleError::Other("Preimage not found".to_string()))
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        let data = self.get(key).await?;
        buf.copy_from_slice(data.as_slice());
        Ok(())
    }
}

#[async_trait::async_trait]
impl <KV: KeyValueStore + ?Sized + Send + Sync > HintWriterClient for PreimageIO<KV> {
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        let (sender, mut receiver) = oneshot::channel();

        self.hint_sender.send((hint.to_string(), sender)).await
         .map_err(|e| PreimageOracleError::Other(e.to_string()))?;

        _ = receiver.await.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        Ok(())
    }
}