use crate::derivation::oracle::Cache;
use alloy_primitives::B256;
use kona_preimage::errors::PreimageOracleResult;
use kona_preimage::{CommsClient, HintWriterClient, PreimageKey, PreimageOracleClient};
use lru::LruCache;
use prost::Message;
use spin::Mutex;
use std::fmt::Debug;
use std::sync::Arc;

pub trait PreimageTraceable {
    fn preimages(&self) -> Vec<u8>;
}

#[derive(Debug, Clone)]
pub struct TracingPreimageIO {
    //TODO to HashMap
    used: Arc<Mutex<Preimages>>,
    cache: Arc<spin::Mutex<LruCache<PreimageKey, Vec<u8>>>>,
}

impl TracingPreimageIO {
    pub fn new(cache: Cache) -> Self {
        Self {
            used: Arc::new(Mutex::new(Preimages::default())),
            cache,
        }
    }
}

impl PreimageTraceable for TracingPreimageIO {
    fn preimages(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        let lock = self.used.lock();
        lock.encode(&mut buf).unwrap();
        buf
    }
}

#[async_trait::async_trait]
impl PreimageOracleClient for TracingPreimageIO {
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let mut cache_lock = self.cache.lock();
        let result = cache_lock.get(&key).unwrap();
        self.used
            .lock()
            .preimages
            .push(Preimage::new(key, result.clone()));
        Ok(result.clone())
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        let mut cache_lock = self.cache.lock();
        let result = cache_lock.get(&key).unwrap();
        buf.copy_from_slice(result.as_slice());
        self.used
            .lock()
            .preimages
            .push(Preimage::new(key, buf.to_vec()));
        Ok(())
    }
}

#[async_trait::async_trait]
impl HintWriterClient for TracingPreimageIO {
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        Ok(())
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
