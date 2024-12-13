//! Contains FPVM-specific constructs for the `kona-client` program.

use kona_preimage::{HintWriter, HintWriterClient, OracleReader, PreimageOracleClient};
use crate::config::Config;
use crate::derivation::oracle::multiprocess::client::{PreimageIO, HINT_WRITER, ORACLE_READER};
use crate::derivation::oracle::new_cache;

mod client;

pub async fn make_oracle(_: &Config) -> PreimageIO<OracleReader, HintWriter>
{
    let cache = new_cache();
    PreimageIO::new(cache, ORACLE_READER, HINT_WRITER)
}