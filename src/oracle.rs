use std::fmt::Debug;
use std::sync::{Arc, Mutex, RwLock};
use hashbrown::HashMap;
use kona_preimage::{HintReaderServer, HintRouter, HintWriterClient, PreimageFetcher, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use kona_host::fetcher::Fetcher;
use kona_host::kv::KeyValueStore;

#[derive(Clone)]
pub struct PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync {
    pub fetcher: Arc<RwLock<Fetcher<KV>>>
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
        self.fetcher.read().unwrap().get_preimage(key.into())
            .await.map_err(|e| PreimageOracleError::Other(e.to_string()))
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        let data = self.fetcher.read().unwrap().get_preimage(key.into())
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
        let mut lock = self.fetcher.write().unwrap();
        lock.hint(hint);
        Ok(())
    }
}