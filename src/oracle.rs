use std::sync::{Arc, Mutex, RwLock};
use hashbrown::HashMap;
use kona_preimage::{HintReaderServer, HintRouter, HintWriterClient, PreimageFetcher, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use kona_host::fetcher::Fetcher;
use kona_host::kv::KeyValueStore;

#[derive(Clone, Debug)]
pub struct PreimageIO<KV> where
    KV: KeyValueStore + ?Sized{
    pub fetcher: Arc<RwLock<Fetcher<KV>>>
}

#[async_trait::async_trait]
impl <KV> PreimageOracleClient for PreimageIO<KV>
where
    KV: KeyValueStore + ?Sized {
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
    KV: KeyValueStore + ?Sized
{
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        let lock = self.fetcher.lock();
        lock.hint(hint);
        Ok(())
    }
}