use alloy_eips::eip4844::FIELD_ELEMENTS_PER_BLOB;
use alloy_primitives::{keccak256, B256};
use hashbrown::HashMap;
use kona_preimage::errors::PreimageOracleResult;
use kona_preimage::{
    CommsClient, HintWriterClient, PreimageKey, PreimageKeyType, PreimageOracleClient,
};
use optimism_derivation::types::{Preimage, Preimages};
use optimism_derivation::POSITION_FIELD_ELEMENT;
use spin::Mutex;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TracingPreimageIO<T>
where
    T: PreimageOracleClient + HintWriterClient + Send + Sync,
{
    used: Arc<Mutex<HashMap<PreimageKey, Vec<u8>>>>,
    inner: T,
}

impl<T> TracingPreimageIO<T>
where
    T: PreimageOracleClient + HintWriterClient + Send + Sync,
{
    pub fn new(inner: T) -> Self {
        Self {
            used: Arc::new(Mutex::new(HashMap::default())),
            inner,
        }
    }

    async fn used(&self, key: PreimageKey, value: Vec<u8>) -> PreimageOracleResult<()> {
        let hash = match key.key_type() {
            PreimageKeyType::Blob => {
                let raw_key = B256::from(key).0;
                let hash_key = PreimageKey::new(raw_key, PreimageKeyType::Keccak256);
                let blob_key = self.inner.get(hash_key).await?;

                let mut last_blob_key = blob_key.clone();
                last_blob_key[POSITION_FIELD_ELEMENT..]
                    .copy_from_slice(FIELD_ELEMENTS_PER_BLOB.to_be_bytes().as_ref());
                let raw_key = keccak256(&last_blob_key);
                let last_blob_key_hash = PreimageKey::new(raw_key.0, PreimageKeyType::Keccak256);
                let last_blob_blob_key = PreimageKey::new(raw_key.0, PreimageKeyType::Blob);
                let all_blob_key = self.inner.get(last_blob_key_hash).await?;
                let kzg_proof = self.inner.get(last_blob_blob_key).await?;
                Some(vec![
                    (hash_key, blob_key),
                    (last_blob_key_hash, all_blob_key),
                    (last_blob_blob_key, kzg_proof),
                ])
            }
            PreimageKeyType::Precompile => {
                let raw_key = B256::from(key).0;
                let hash_key = PreimageKey::new(raw_key, PreimageKeyType::Keccak256);
                let result = self.inner.get(hash_key).await?;
                Some(vec![(hash_key, result)])
            }
            _ => None,
        };
        let mut lock = self.used.lock();
        if let Some(hash) = hash {
            for hash in hash {
                lock.insert(hash.0, hash.1);
            }
        }
        lock.insert(key, value);
        Ok(())
    }
}

impl<T> From<TracingPreimageIO<T>> for Vec<u8>
where
    T: PreimageOracleClient + HintWriterClient + Send + Sync,
{
    fn from(value: TracingPreimageIO<T>) -> Self {
        let used = value.used.lock();
        let mut temp: Vec<Preimage> = Vec::with_capacity(used.len());
        for (k, v) in used.iter() {
            temp.push(Preimage::new(k.clone(), v.clone()));
        }
        let data = Preimages { preimages: temp };
        data.into_vec().unwrap()
    }
}

#[async_trait::async_trait]
impl<T> PreimageOracleClient for TracingPreimageIO<T>
where
    T: PreimageOracleClient + HintWriterClient + Send + Sync,
{
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let result = self.inner.get(key.clone()).await?;
        self.used(key, result.clone()).await?;
        Ok(result)
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        self.inner.get_exact(key.clone(), buf).await?;
        self.used(key, buf.to_vec()).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<T> HintWriterClient for TracingPreimageIO<T>
where
    T: PreimageOracleClient + HintWriterClient + Send + Sync,
{
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        self.inner.write(hint).await
    }
}
