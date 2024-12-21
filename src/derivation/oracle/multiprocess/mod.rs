//! Contains FPVM-specific constructs for the `kona-client` program.

use crate::config::Config;
use crate::derivation::oracle::multiprocess::client::{PreimageIO, HINT_WRITER, ORACLE_READER};
use crate::derivation::oracle::new_cache;
use kona_preimage::{HintWriter, HintWriterClient, OracleReader, PreimageOracleClient};

mod client;

pub async fn make_oracle(_: &Config) -> PreimageIO<OracleReader, HintWriter> {
    let cache = new_cache();
    PreimageIO::new(cache, ORACLE_READER, HINT_WRITER)
}
