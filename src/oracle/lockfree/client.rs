use std::fmt::Debug;
use alloy_primitives::B256;
use kona_preimage::{HintWriterClient, PreimageFetcher, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use tokio::sync::{oneshot, RwLock};
use crate::oracle::lockfree::{HintSender, PreimageSender};

#[derive(Clone, Debug)]
pub struct PreimageIO {
    hint_sender: async_channel::Sender<(String, HintSender)>,
    preimage_sender: async_channel::Sender<(B256, PreimageSender)>,
}

impl PreimageIO {
    pub fn new(
        hint_sender: async_channel::Sender<(String, HintSender)>,
        preimage_sender: async_channel::Sender<(B256, PreimageSender)>
    ) -> Self {
        Self {
            hint_sender,
            preimage_sender,
        }
    }
}

#[async_trait::async_trait]
impl PreimageOracleClient for PreimageIO {
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let (sender, mut receiver) = oneshot::channel();

        self.preimage_sender.send((key.into(), sender)).await
            .map_err(|e| PreimageOracleError::Other(e.to_string()))?;

        let result = receiver.await.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        let result = result.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
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

        self.hint_sender.send((hint.to_string(), sender)).await
         .map_err(|e| PreimageOracleError::Other(e.to_string()))?;

        _ = receiver.await.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        Ok(())
    }
}