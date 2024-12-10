use std::fmt::Debug;
use std::sync::Arc;
use alloy_primitives::B256;
use kona_preimage::{CommsClient, HintWriterClient, PreimageKey, PreimageOracleClient};
use kona_preimage::errors::PreimageOracleResult;
use prost::Message;
use spin::Mutex;

pub trait PreimageTraceable {
    fn preimages(&self) -> Vec<u8>;
}

#[derive(Debug, Clone)]
pub struct PreimageIO<OR, HW>
where
    OR: PreimageOracleClient,
    HW: HintWriterClient,
{
    used: Arc<Mutex<Preimages>>,
    /// Oracle reader type.
    oracle_reader: OR,
    /// Hint writer type.
    hint_writer: HW,
}

impl <OR,HW> PreimageIO<OR,HW>
where
    OR: PreimageOracleClient,
    HW: HintWriterClient,
{
    pub fn new(oracle_reader: OR, hint_writer: HW) -> Self {
        Self {
            used: Arc::new(Mutex::new(Preimages::default())),
            oracle_reader,
            hint_writer
        }
    }
}

impl <OR,HW> PreimageTraceable for PreimageIO<OR,HW>
where
    OR: PreimageOracleClient,
    HW: HintWriterClient,
{
    fn preimages(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        let lock = self.used.lock();
        lock.encode(&mut buf).unwrap();
        buf
    }
}

#[async_trait::async_trait]
impl <OR,HW> PreimageOracleClient for PreimageIO<OR,HW>
where
    OR: PreimageOracleClient + Sync,
    HW: HintWriterClient + Sync,
{
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let result = self.oracle_reader.get(key).await?;
        self.used.lock().preimages.push(Preimage::new(key, result.clone()));
        Ok(result)
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        self.oracle_reader.get_exact(key, buf).await?;
        self.used.lock().preimages.push(Preimage::new(key, buf.to_vec()));
        Ok(())
    }
}

#[async_trait::async_trait]
impl <OR,HW> HintWriterClient for PreimageIO<OR,HW>
where
    OR: PreimageOracleClient + Sync,
    HW: HintWriterClient + Sync,
{
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        self.hint_writer.write(hint).await
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