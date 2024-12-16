use crate::derivation::oracle::Cache;
use alloy_primitives::B256;
use hashbrown::HashMap;
use kona_preimage::errors::PreimageOracleResult;
use kona_preimage::{CommsClient, HintWriterClient, PreimageKey, PreimageOracleClient};
use lru::LruCache;
use prost::Message;
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
        let mut buf: Vec<u8> = Vec::new();
        data.encode(&mut buf).unwrap();
        buf
    }
}

#[async_trait::async_trait]
impl<T> PreimageOracleClient for TracingPreimageIO<T>
where
    T: PreimageOracleClient + HintWriterClient + Send + Sync,
{
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let result = self.inner.get(key).await?;
        // TODO save hashkey if the key type is blob or precompiles
        self.used.lock().insert(key, result.clone());
        Ok(result.clone())
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        self.inner.get_exact(key, buf).await?;
        self.used.lock().insert(key, buf.to_vec());
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

#[derive(::prost::Message, Clone, PartialEq)]
pub struct Preimage {
    #[prost(bytes = "vec", tag = "1")]
    pub key: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub data: Vec<u8>,
}

impl Preimage {
    pub fn new(key: PreimageKey, data: Vec<u8>) -> Self {
        Self {
            key: B256::from(key).0.to_vec(),
            data,
        }
    }
}

#[derive(::prost::Message, Clone, PartialEq)]
pub struct Preimages {
    #[prost(message, repeated, tag = "1")]
    pub preimages: Vec<Preimage>,
}

#[cfg(test)]
mod test {
    use crate::webapp::oracle::{Preimage, Preimages};
    use prost::Message;

    #[test]
    pub fn test_preimage_encode_decode() {
        let expected = Preimage {
            key: vec![1, 2, 3],
            data: vec![4, 5, 6],
        };
        let mut buf: Vec<u8> = Vec::new();
        expected.encode(&mut buf).unwrap();

        let actual = Preimage::decode(&*buf).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    pub fn test_preimages_encode_decode() {
        let expected = Preimages {
            preimages: vec![
                Preimage {
                    key: vec![1, 2, 3],
                    data: vec![4, 5, 6],
                },
                Preimage {
                    key: vec![7, 8, 9],
                    data: vec![10, 11, 12],
                },
            ],
        };
        let mut buf: Vec<u8> = Vec::new();
        expected.encode(&mut buf).unwrap();

        let actual = Preimages::decode(&*buf).unwrap();
        assert_eq!(expected, actual);
    }
}
