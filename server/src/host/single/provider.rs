use std::sync::Arc;
use alloy_eips::eip4844::IndexedBlobHash;
use alloy_provider::RootProvider;
use alloy_rpc_types_beacon::sidecar::BlobData;
use axum::async_trait;
use kona_host::OnlineHostBackendCfg;
use kona_host::single::{SingleChainHost, SingleChainProviders};
use kona_proof::HintType;
use moka::future::Cache as MokaCache;
use kona_providers_alloy::{APIConfigResponse, APIGenesisResponse, BeaconClient, BlobSidecarProvider, OnlineBeaconClient, OnlineBlobProvider};
use op_alloy_network::Optimism;

pub type Cache = MokaCache<(u64, Vec<IndexedBlobHash>), Vec<BlobData>>;
#[derive(Debug, Clone)]
pub struct OnlineBeaconClientProxy {
    inner : OnlineBeaconClient,
    cache: Cache
}

impl OnlineBeaconClientProxy {
    /// Creates a new instance of the [OnlineBeaconClientProxy].
    pub fn new(inner: OnlineBeaconClient, cache: Cache) -> Self {
        Self {
            inner,
            cache,
        }
    }
}

#[async_trait]
impl BeaconClient for OnlineBeaconClientProxy {
    type Error = reqwest::Error;

    async fn config_spec(&self) -> Result<APIConfigResponse, Self::Error> {
        self.inner.config_spec()
    }

    async fn beacon_genesis(&self) -> Result<APIGenesisResponse, Self::Error> {
        self.inner.beacon_genesis()
    }

    async fn beacon_blob_side_cars(
        &self,
        slot: u64,
        hashes: &[IndexedBlobHash],
    ) -> Result<Vec<BlobData>, Self::Error> {
        let key = (slot, hashes.to_vec());

        if let Some(cached) = self.cache.get(&key).await {
            return Ok(cached.clone());
        }
        let sidecars = self.inner.beacon_blob_side_cars(slot, hashes).await?;
        self.cache.insert(key, sidecars.clone()).await;
        Ok(sidecars)
    }
}
