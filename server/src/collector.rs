use std::sync::Arc;
use alloy_primitives::B256;
use tokio::select;
use tracing::{error, info, metadata};
use crate::client::l2_client::{L2Client};
use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
use crate::derivation::host::single::handler::{Derivation, DerivationConfig, DerivationRequest};

pub struct PreimageCollector<T: PreimageRepository> {
    pub client: L2Client,
    pub config: DerivationConfig,
    pub chunk: u64,
    pub initial_claimed: u64,
    pub preimage_repository: Arc<T>
}

impl <T: PreimageRepository> PreimageCollector<T> {
    pub async fn start(&self) {

        let mut latest_l2: u64 = match self.preimage_repository.latest_metadata().await {
            Some(metadata) => metadata.claimed,
            None => self.initial_claimed
        };
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;

            // Check sync status
            let sync_status = self.client.sync_status().await;
            let sync_status = match sync_status {
                Ok(sync_status) => {
                    info!("sync status: claimed_l2={}, next_claiming_l2={}, finalized_l1={}", latest_l2, sync_status.finalized_l2.number, sync_status.finalized_l1.number);
                    if sync_status.finalized_l2.number <= latest_l2 {
                        continue;
                    }
                    sync_status
                }
                Err(e) => {
                    error!("Failed to get sync status {:?}", e);
                    continue;
                },
            };

            // Collect preimage from latest_l2 to finalized_l2
            let pairs = split(latest_l2, sync_status.finalized_l2.number, self.chunk);
            info!("derivation length={}, target={:?}", pairs.len(), pairs);
            for (start, end) in pairs {
                match self.collect(sync_status.finalized_l1.hash, start, end).await {
                    Ok(_) =>  {
                        info!("saving preimage success, claimed_l2={end}");
                        latest_l2 = end;
                    },
                    Err(e) => {
                        error!("Failed to collect preimage {:?}", e);
                        // restart from latest_l2
                        break;
                    }
                }
            }
         }
    }


    async fn collect(&self, l1_head_hash: B256, start: u64, end: u64) -> anyhow::Result<()> {
        let agreed_block = self.client.output_root_at(start).await?;
        let claiming_block = self.client.output_root_at(end).await?;

        let request = DerivationRequest {
            l1_head_hash,
            agreed_l2_head_hash: agreed_block.block_ref.hash,
            agreed_l2_output_root: agreed_block.output_root,
            l2_output_root: claiming_block.output_root,
            l2_block_number: end,
        };

        info!("derivation start : {:?}", &request);
        let derivation = Derivation {
            config: self.config.clone(),
            request,
        };
        let preimage = derivation.start().await?;
        let metadata = PreimageMetadata { agreed: start, claimed: end, l1_head: l1_head_hash };
        info!("derivation success : {metadata:?}");
        self.preimage_repository.upsert(metadata, preimage).await
    }
}

fn split(agreed: u64, finalized: u64, chunk: u64) -> Vec<(u64, u64)> {
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    let mut start = agreed;
    while start < finalized {
        let end = std::cmp::min(start + chunk, finalized);
        pairs.push((start, end));
        start = end;
    }
    pairs
}
