use crate::derivation::oracle::r#unsafe::fetcher::Fetcher;
use kona_host::kv::KeyValueStore;
use kona_preimage::errors::{PreimageOracleError, PreimageOracleResult};
use kona_preimage::{HintWriterClient, PreimageFetcher, PreimageKey, PreimageOracleClient};
use std::fmt::Debug;
use std::sync::Arc;

pub struct PreimageIO<KV: KeyValueStore + ?Sized + Send + Sync> {
    fetcher: Arc<Fetcher<KV>>,
}

impl<KV: KeyValueStore + ?Sized + Send + Sync> Debug for PreimageIO<KV> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreimageIO").finish()
    }
}

impl<KV> Clone for PreimageIO<KV>
where
    KV: KeyValueStore + ?Sized + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            fetcher: self.fetcher.clone(),
        }
    }
}

impl<KV: KeyValueStore + ?Sized + Send + Sync> PreimageIO<KV> {
    pub fn new(fetcher: Arc<Fetcher<KV>>) -> Self {
        Self { fetcher }
    }
}

#[async_trait::async_trait]
impl<KV: KeyValueStore + ?Sized + Send + Sync> PreimageOracleClient for PreimageIO<KV> {
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        self.fetcher
            .get_preimage(key.into())
            .await
            .map_err(|e| PreimageOracleError::Other(e.to_string()))
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        let data = self.get(key).await?;
        buf.copy_from_slice(data.as_slice());
        Ok(())
    }
}

#[async_trait::async_trait]
impl<KV: KeyValueStore + ?Sized + Send + Sync> HintWriterClient for PreimageIO<KV> {
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        self.fetcher.hint(hint);
        Ok(())
    }
}
