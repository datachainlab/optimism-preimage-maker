//! Contains the [CachingOracle], which is a wrapper around an [OracleReader] and [HintWriter] that
//! stores a configurable number of responses in an [LruCache] for quick retrieval.
//!
//! [OracleReader]: kona_preimage::OracleReader
//! [HintWriter]: kona_preimage::HintWriter
use crate::derivation::oracle::Cache;
use async_trait::async_trait;
use kona_common::FileDescriptor;
use kona_preimage::errors::PreimageOracleResult;
use kona_preimage::{
    HintWriter, HintWriterClient, OracleReader, PipeHandle, PreimageKey, PreimageOracleClient,
};

/// The global preimage oracle reader pipe.
static ORACLE_READER_PIPE: PipeHandle =
    PipeHandle::new(FileDescriptor::PreimageRead, FileDescriptor::PreimageWrite);

/// The global hint writer pipe.
static HINT_WRITER_PIPE: PipeHandle =
    PipeHandle::new(FileDescriptor::HintRead, FileDescriptor::HintWrite);

/// The global preimage oracle reader.
pub static ORACLE_READER: OracleReader = OracleReader::new(ORACLE_READER_PIPE);

/// The global hint writer.
pub static HINT_WRITER: HintWriter = HintWriter::new(HINT_WRITER_PIPE);

/// A wrapper around an [OracleReader] and [HintWriter] that stores a configurable number of
/// responses in an [LruCache] for quick retrieval.
///
/// [OracleReader]: kona_preimage::OracleReader
/// [HintWriter]: kona_preimage::HintWriter
#[allow(unreachable_pub)]
#[derive(Debug, Clone)]
pub struct PreimageIO<OR, HW>
{
    /// The spin-locked cache that stores the responses from the oracle.
    pub cache: Cache,
    /// Oracle reader type.
    oracle_reader: OR,
    /// Hint writer type.
    hint_writer: HW,
}

impl<OR, HW> PreimageIO<OR, HW>
where
    OR: PreimageOracleClient,
    HW: HintWriterClient,
{
    /// Creates a new [CachingOracle] that wraps the given [OracleReader] and stores up to `N`
    /// responses in the cache.
    ///
    /// [OracleReader]: kona_preimage::OracleReader
    pub fn new(cache: Cache, oracle_reader: OR, hint_writer: HW) -> Self {
        Self {
            cache,
            oracle_reader,
            hint_writer,
        }
    }
}

#[async_trait]
impl<OR, HW> PreimageOracleClient for PreimageIO<OR, HW>
where
    OR: PreimageOracleClient + Sync,
    HW: HintWriterClient + Sync,
{
    async fn get(&self, key: PreimageKey) -> PreimageOracleResult<Vec<u8>> {
        let mut cache_lock = self.cache.lock();
        if let Some(value) = cache_lock.get(&key) {
            Ok(value.clone())
        } else {
            let value = self.oracle_reader.get(key).await?;
            cache_lock.put(key, value.clone());
            Ok(value)
        }
    }

    async fn get_exact(&self, key: PreimageKey, buf: &mut [u8]) -> PreimageOracleResult<()> {
        let mut cache_lock = self.cache.lock();
        if let Some(value) = cache_lock.get(&key) {
            // SAFETY: The value never enters the cache unless the preimage length matches the
            // buffer length, due to the checks in the OracleReader.
            buf.copy_from_slice(value.as_slice());
            Ok(())
        } else {
            self.oracle_reader.get_exact(key, buf).await?;
            cache_lock.put(key, buf.to_vec());
            Ok(())
        }
    }
}

#[async_trait]
impl<OR, HW> HintWriterClient for PreimageIO<OR, HW>
where
    OR: PreimageOracleClient + Sync,
    HW: HintWriterClient + Sync,
{
    async fn write(&self, hint: &str) -> PreimageOracleResult<()> {
        self.hint_writer.write(hint).await
    }
}
