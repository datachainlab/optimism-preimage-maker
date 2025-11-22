use std::sync::Arc;
use alloy_primitives::B256;
use tokio::select;
use tracing::{error, info, metadata};
use crate::client::l2_client::{L2Client};
use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
use crate::derivation::host::single::handler::{Derivation, DerivationConfig, DerivationRequest};

pub struct PreimageCollector<T: PreimageRepository> {
    pub client: Arc<L2Client>,
    pub config: Arc<DerivationConfig>,
    pub preimage_repository: Arc<T>,
    pub max_distance: u64,
    pub max_concurrency: usize,
    pub initial_claimed: u64,
    pub interval_seconds: u64,
}

impl <T: PreimageRepository> PreimageCollector<T> {
    pub async fn start(&self) {
        let mut latest_l2: u64 = match self.preimage_repository.latest_metadata().await {
            Some(metadata) => metadata.claimed,
            None => self.initial_claimed
        };
        loop {
            if let Some(claimed) = self.collect(latest_l2).await {
                latest_l2 = claimed;
            }
            tokio::time::sleep(std::time::Duration::from_secs(self.interval_seconds)).await;
        }
    }

    async fn collect(&self, latest_l2: u64) -> Option<u64> {
        let mut latest_l2 = latest_l2;
        // Check sync status
        let sync_status = self.client.sync_status().await;
        let sync_status = match sync_status {
            Ok(sync_status) => {
                info!("sync status: claimed_l2={}, next_claiming_l2={}, finalized_l1={}", latest_l2, sync_status.finalized_l2.number, sync_status.finalized_l1.number);
                if sync_status.finalized_l2.number <= latest_l2 {
                    return None;
                }
                sync_status
            }
            Err(e) => {
                error!("Failed to get sync status {:?}", e);
                return None;
            },
        };

        // Collect preimage from latest_l2 to finalized_l2
        let pairs = split(latest_l2, sync_status.finalized_l2.number, self.max_distance);
        let batches = pairs.chunks(self.max_concurrency.max(1));
        info!("derivation length={}, batch-size={}, target={:?}", pairs.len(), batches.len(), batches);
        for batch in batches {
            let l1_head_hash = sync_status.finalized_l1.hash;
            match self.parallel_collect(l1_head_hash, batch.to_vec()).await {
                Ok(end) => latest_l2 = end.unwrap_or(latest_l2),
                Err(e) => {
                    error!("Failed to collect preimage in current batch. try collect from {}: {:?}", latest_l2, e);
                    break;
                }
            }
        }
        Some(latest_l2)
    }

    async fn parallel_collect(&self, l1_head_hash: B256, batch: Vec<(u64, u64)>) -> anyhow::Result<Option<u64>> {
        let mut tasks = vec![];

        // Spawn tasks to collect preimages
        for (start, end) in batch {
            let l2_client = self.client.clone();
            let config = self.config.clone();
            tasks.push(tokio::spawn(async move {
                collect(l2_client, config, l1_head_hash, start, end).await
            }));
        }

        // Wait for all tasks to finish
        let mut results = vec![];
        for task in tasks {
            results.push(task.await);
        }

        let mut latest_l2 = None;
        for result in results {
            // preimage must be sequentially stored in db, so we can restart from latest_l2 when failed to save preimage.
            let result = result??;
            // Commit preimages to db
            let (metadata, preimage) = result;
            let claimed = metadata.claimed;
            self.preimage_repository.upsert(metadata, preimage).await?;
            latest_l2 = Some(claimed);
        }
        Ok(latest_l2)
    }

}


async fn collect(client: Arc<L2Client>, config: Arc<DerivationConfig>, l1_head_hash: B256, start: u64, end: u64) -> anyhow::Result<(PreimageMetadata, Vec<u8>)> {
    let agreed_block = client.output_root_at(start).await?;
    let claiming_block = client.output_root_at(end).await?;

    let request = DerivationRequest {
        l1_head_hash,
        agreed_l2_head_hash: agreed_block.block_ref.hash,
        agreed_l2_output_root: agreed_block.output_root,
        l2_output_root: claiming_block.output_root,
        l2_block_number: end,
    };

    info!("derivation start : {:?}", &request);
    let derivation = Derivation {
        config,
        request,
    };
    let metadata = PreimageMetadata { agreed: start, claimed: end, l1_head: l1_head_hash };
    let result = derivation.start().await;
    match result {
       Ok(preimage) => {
           info!("derivation success : {metadata:?}");
           Ok((metadata, preimage))
       },
        Err(e) => {
           error!("derivation failed : {metadata:?}, error={}", e);
           Err(e)
        }
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
