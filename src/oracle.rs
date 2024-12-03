use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use kona_preimage::{HintWriterClient, PreimageFetcher, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use kona_host::kv::KeyValueStore;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use crate::fetcher::Fetcher;

pub struct PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync  {
    fetcher: Arc<Fetcher<KV>>,
    sender: tokio::sync::mpsc::UnboundedSender<(String, oneshot::Sender<(bool)>)>,
}

impl <KV> PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync {
    pub fn new(fetcher: Arc<Fetcher<KV>>, sender:  tokio::sync::mpsc::UnboundedSender<(String, oneshot::Sender<(bool)>)>) -> Self {
        Self {
            fetcher,
            sender,
        }
    }
}

impl <KV> Clone for PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync {
    fn clone(&self) -> Self {
        Self {
            fetcher: self.fetcher.clone(),
            sender: self.sender.clone(),
        }
    }
}

impl <KV> Debug for PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreimageIO")
            .finish()
    }
}

#[async_trait::async_trait]
impl <KV> PreimageOracleClient for PreimageIO<KV>
where
    KV: KeyValueStore + ?Sized + Send + Sync{
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let v = self.fetcher.get_preimage(key.into())
            .await.map_err(|e| PreimageOracleError::Other(e.to_string()));
        v
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        let data = self.fetcher.get_preimage(key.into())
            .await.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        buf.copy_from_slice(data.as_slice());
        Ok(())
    }
}

#[async_trait::async_trait]
impl <KV> HintWriterClient for PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync
{
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        //let (sender, mut receiver) = oneshot::channel();

        //self.sender.send((hint.to_string(), sender))
         //   .map_err(|e| PreimageOracleError::Other(e.to_string()))?;

        //let result = receiver.await.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        self.fetcher.prefetch(hint).await.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        Ok(())
    }
}