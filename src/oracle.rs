use std::fmt::Debug;
use std::sync::{Arc,};
use kona_preimage::{HintWriterClient, PreimageFetcher, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use kona_host::kv::KeyValueStore;
use crate::fetcher::Fetcher;

pub struct PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync  {
    fetcher: Arc<Fetcher<KV>>
}

impl <KV> PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync {
    pub fn new(fetcher: Fetcher<KV>) -> Self {
        Self {
            fetcher: Arc::new(fetcher)
        }
    }
}

impl <KV> Clone for PreimageIO<KV> where
    KV: KeyValueStore + ?Sized + Send + Sync {
    fn clone(&self) -> Self {
        Self {
            fetcher: self.fetcher.clone()
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
        self.fetcher.get_preimage(key.into())
            .await.map_err(|e| PreimageOracleError::Other(e.to_string()))
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
        // TODO 毎回再取得が実行されるのでとても遅い。ヒント毎にキャッシュが必要
        self.fetcher.prefetch(hint).await.map_err(|e| PreimageOracleError::Other(e.to_string()))?;
        Ok(())
    }
}