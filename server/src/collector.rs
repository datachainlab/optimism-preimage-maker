use crate::client::l2_client::L2Client;
use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
use crate::derivation::host::single::handler::{Derivation, DerivationConfig, DerivationRequest};
use alloy_primitives::B256;
use std::sync::Arc;
use tokio::time;
use tracing::{error, info, warn};
use crate::client::beacon_client::{BeaconClient, LightClientFinalityUpdateResponse};
use crate::data::finalized_l1_repository::FinalizedL1Repository;

pub struct PreimageCollector<T: PreimageRepository, F: FinalizedL1Repository> {
    pub client: Arc<L2Client>,
    pub beacon_client: Arc<BeaconClient>,
    pub config: Arc<DerivationConfig>,
    pub preimage_repository: Arc<T>,
    pub finalized_l1_repository: Arc<F>,
    pub max_distance: u64,
    pub max_concurrency: usize,
    pub initial_claimed: u64,
    pub interval_seconds: u64,
}

impl<T: PreimageRepository, F: FinalizedL1Repository> PreimageCollector<T, F> {
    pub async fn start(&self) {
        let mut latest_l2: u64 = match self.preimage_repository.latest_metadata().await {
            Some(metadata) => metadata.claimed,
            None => self.initial_claimed,
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
                info!(
                    "sync status: claimed_l2={}, next_claiming_l2={}, sync_finalized_l1={}, processed_l2={}",
                    latest_l2, sync_status.finalized_l2.number, sync_status.finalized_l1.number, latest_l2
                );
                if sync_status.finalized_l2.number <= latest_l2 {
                    return None;
                }
                sync_status
            }
            Err(e) => {
                error!("Failed to get sync status {:?}", e);
                return None;
            }
        };

        let (finality_l1 ,raw_finality_l1) = loop {
            let raw_finality_l1 = match self.beacon_client.get_raw_light_client_finality_update().await {
                Ok(finality_l1) => finality_l1,
                Err(e) => {
                    error!("Failed to get finality update from beacon client {:?}", e);
                    return None;
                }
            };
            let finality_l1 : LightClientFinalityUpdateResponse = match serde_json::from_str(&raw_finality_l1) {
                Ok(value) => value,
                Err(e) => {
                    error!("Failed to get finality update from beacon client {:?}", e);
                    return None;
                }
            };
            let block_number = finality_l1.data.finalized_header.execution.block_number;
            if block_number < sync_status.finalized_l1.number {
                warn!("finality_l1 = {:?} delayed.", block_number);
                time::sleep(time::Duration::from_secs(10)).await;
                continue
            }
            break (finality_l1, raw_finality_l1)

        };
        let l1_head_hash = finality_l1.data.finalized_header.execution.block_hash;
        info!("l1_head for derivation = {:?}",finality_l1.data.finalized_header.execution);

        // Collect preimage from latest_l2 to finalized_l2
        let pairs = split(
            latest_l2,
            sync_status.finalized_l2.number,
            self.max_distance,
        );
        let batches = pairs.chunks(self.max_concurrency.max(1));
        info!(
            "derivation length={}, batch-size={}, target={:?}",
            pairs.len(),
            batches.len(),
            batches
        );

        // Save finalized_l1
        if let Err(e) = self.finalized_l1_repository.upsert(&l1_head_hash, raw_finality_l1).await {
            error!("Failed to save finalized l1 head hash to db l1_head={}, {:?}", l1_head_hash, e);
        }

        for batch in batches {
            match self.parallel_collect(l1_head_hash.clone(), batch.to_vec()).await {
                Ok(end) => latest_l2 = end.unwrap_or(latest_l2),
                Err(e) => {
                    error!(
                        "Failed to collect preimage in current batch. try collect from {}: {:?}",
                        latest_l2, e
                    );
                    break;
                }
            }
        }

        Some(latest_l2)
    }

    async fn parallel_collect(
        &self,
        l1_head_hash: B256,
        batch: Vec<(u64, u64)>,
    ) -> anyhow::Result<Option<u64>> {
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

        // Commit
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

async fn collect(
    client: Arc<L2Client>,
    config: Arc<DerivationConfig>,
    l1_head_hash: B256,
    start: u64,
    end: u64,
) -> anyhow::Result<(PreimageMetadata, Vec<u8>)> {
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
    let derivation = Derivation { config, request };
    let metadata = PreimageMetadata {
        agreed: start,
        claimed: end,
        l1_head: l1_head_hash,
    };
    let result = derivation.start().await;
    match result {
        Ok(preimage) => {
            info!("derivation success : {metadata:?}");
            Ok((metadata, preimage))
        }
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

#[cfg(test)]
mod tests {
    use super::split;

    fn assert_contiguous(pairs: &[(u64, u64)], agreed: u64, finalized: u64) {
        if pairs.is_empty() {
            assert!(
                agreed >= finalized || agreed == finalized,
                "empty implies agreed==finalized"
            );
            return;
        }
        assert_eq!(
            pairs.first().unwrap().0,
            agreed,
            "first start must equal agreed"
        );
        assert_eq!(
            pairs.last().unwrap().1,
            finalized,
            "last end must equal finalized"
        );
        for w in pairs.windows(2) {
            let (s1, e1) = w[0];
            let (s2, e2) = w[1];
            assert!(s1 < e1, "each segment must be non-empty");
            assert_eq!(
                e1, s2,
                "segments must be contiguous without gaps or overlaps"
            );
            assert!(e2 <= finalized);
        }
    }

    #[test]
    fn test_split_empty_range() {
        let agreed = 10;
        let finalized = 10;
        let chunk = 5;
        let pairs = split(agreed, finalized, chunk);
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_split_single_chunk_when_chunk_bigger_than_range() {
        let agreed = 5;
        let finalized = 8; // range size = 3
        let chunk = 10; // larger than range
        let pairs = split(agreed, finalized, chunk);
        assert_eq!(pairs, vec![(5, 8)]);
        assert_contiguous(&pairs, agreed, finalized);
    }

    #[test]
    fn test_split_exact_multiples() {
        let agreed = 0;
        let finalized = 10;
        let chunk = 2;
        let pairs = split(agreed, finalized, chunk);
        assert_eq!(pairs, vec![(0, 2), (2, 4), (4, 6), (6, 8), (8, 10)]);
        assert_contiguous(&pairs, agreed, finalized);
    }

    #[test]
    fn test_split_non_exact_final_chunk() {
        let agreed = 3;
        let finalized = 11; // range size = 8
        let chunk = 4;
        let pairs = split(agreed, finalized, chunk);
        assert_eq!(pairs, vec![(3, 7), (7, 11)]);
        assert_contiguous(&pairs, agreed, finalized);
    }

    #[test]
    fn test_split_chunk_one() {
        let agreed = 2;
        let finalized = 5;
        let chunk = 1;
        let pairs = split(agreed, finalized, chunk);
        assert_eq!(pairs, vec![(2, 3), (3, 4), (4, 5)]);
        assert_contiguous(&pairs, agreed, finalized);
    }

    #[test]
    fn test_split_large_values() {
        let agreed = u64::MAX - 9;
        let finalized = u64::MAX; // exclusive upper bound, so last pair should end here
        let chunk = 3;
        let pairs = split(agreed, finalized, chunk);
        // expect: (MAX-9, MAX-6), (MAX-6, MAX-3), (MAX-3, MAX)
        assert_eq!(
            pairs,
            vec![
                (u64::MAX - 9, u64::MAX - 6),
                (u64::MAX - 6, u64::MAX - 3),
                (u64::MAX - 3, u64::MAX),
            ]
        );
        assert_contiguous(&pairs, agreed, finalized);
    }
}
