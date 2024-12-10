use std::fmt::Debug;
use std::sync::Arc;
use alloy_rlp::Encodable;
use kona_preimage::{CommsClient, HintWriterClient, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::PreimageOracleResult;
use spin::Mutex;

pub trait PreimageTraceable {
    fn preimages(&self) -> Vec<u8>;
}

#[derive(Debug, Clone)]
pub struct PreimageIO<T>
where T: CommsClient + Debug + Send + Sync
{
    used: Arc<Mutex<Vec<Preimage>>>,
    inner: Arc<T>
}

impl <T> PreimageIO<T>
where T: CommsClient+ Debug + Send + Sync
{
    pub fn new(inner: Arc<T>) -> Self {
        Self {
            used: Arc::new(Mutex::new(Vec::new())),
            inner
        }
    }
}

impl <T> PreimageTraceable for PreimageIO<T>
where T:CommsClient + Debug + Send + Sync
{
    fn preimages(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        self.used.lock().encode(&mut buf);
        buf
    }
}

#[async_trait::async_trait]
impl <T> PreimageOracleClient for PreimageIO<T>
where T:CommsClient+ Debug + Send + Sync
{
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let result = self.inner.get(key).await?;
        self.used.lock().push(Preimage::new(key, result.clone()));
        Ok(result)
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        self.inner.get_exact(key, buf).await?;
        self.used.lock().push(Preimage::new(key, buf.clone()));
        Ok(())
    }
}

impl <T> HintWriterClient for PreimageIO<T>
where T: CommsClient + Debug + Send + Sync
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
            key: key.into(),
            data
        }
    }

}

#[derive(::prost::Message, Clone, PartialEq)]
pub struct Preimages {
    #[prost(message, repeated, tag = "1")]
    pub preimages: Vec<Preimage>
}

#[cfg(test)]
mod test {
    use prost::Message;
    use crate::handler::oracle::{Preimage, Preimages};

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
            ]
        };
        let mut buf: Vec<u8> = Vec::new();
        expected.encode(&mut buf).unwrap();

        let actual = Preimages::decode(&*buf).unwrap();
        assert_eq!(expected, actual);

    }
}