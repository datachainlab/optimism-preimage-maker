use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use kona_host::KeyValueStore;
use kona_preimage::{HintWriterClient, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use kona_proof::FlushableCache;
use tokio::sync::RwLock;
use crate::host::single::trace::TracingKeyValueStore;

#[derive(Clone)]
pub struct TestOracleClient {
    pub preimages: Arc<RwLock<TracingKeyValueStore>>,
}

impl Debug for TestOracleClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl FlushableCache for TestOracleClient {
    fn flush(&self) {}
}

#[async_trait::async_trait]
impl PreimageOracleClient for TestOracleClient {
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        if let Some(value) = self.preimages.read().await.get(key.into()) {
            Ok(value.clone())
        } else {
            Err(PreimageOracleError::Other(format!(
                "key not found: {:?}",
                key
            )))
        }
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        if let Some(value) = self.preimages.read().await.get(key.into()) {
            buf.copy_from_slice(value.as_slice());
            Ok(())
        } else {
            Err(PreimageOracleError::Other(format!(
                "key not found: {:?}",
                key
            )))
        }
    }
}

#[async_trait::async_trait]
impl HintWriterClient for TestOracleClient {
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
   //     tracing::info!("hint {}", hint);
        Ok(())
    }
}
