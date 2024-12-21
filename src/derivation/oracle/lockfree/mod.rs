use crate::config::Config;
use crate::derivation::oracle::lockfree::client::PreimageIO;
use crate::derivation::oracle::lockfree::fetcher::Fetcher;
use crate::derivation::oracle::lockfree::server::{start_hint_server, start_preimage_server};
use crate::derivation::oracle::new_cache;
use anyhow::Result;
use kona_preimage::{HintWriterClient, PreimageOracleClient};
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};

type PreimageSender = oneshot::Sender<Result<Vec<u8>>>;
type HintSender = oneshot::Sender<bool>;

mod client;
mod fetcher;
mod server;

pub async fn make_oracle(config: &Config) -> PreimageIO {
    let global_kv_store = config.construct_kv_store();
    let (l1_provider, blob_provider, l2_provider) = config.create_providers().await.unwrap();
    let fetcher = Fetcher::new(
        global_kv_store.clone(),
        l1_provider.clone(),
        blob_provider.clone(),
        l2_provider.clone(),
    );
    let fetcher = Arc::new(RwLock::new(fetcher));
    let (hint_channel, _) = start_hint_server(fetcher.clone());
    let (preimage_channel, _) = start_preimage_server(fetcher.clone());
    PreimageIO::new(new_cache(), hint_channel, preimage_channel)
}
