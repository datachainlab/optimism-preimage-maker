use crate::client::beacon_client::{BeaconClient, LightClientFinalityUpdateResponse};
use crate::client::l2_client::{L2Client, SyncStatus};
use crate::data::finalized_l1_repository::FinalizedL1Repository;
use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
use crate::derivation::host::single::handler::{Derivation, DerivationConfig, DerivationRequest};
use alloy_primitives::B256;
use std::sync::Arc;
use tokio::time;
use tracing::{error, info, warn};

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

    /// Starts the asynchronous process to continually check and collect claimed metadata.
    ///
    /// This function retrieves the latest claimed metadata from the `preimage_repository`.
    /// It then enters an infinite loop where it performs the following tasks:
    /// 1. Collects metadata using the `collect` method with the `latest_l2` value.
    /// 2. Updates the `latest_l2` value if new metadata is claimed.
    /// 3. Waits for a specified interval (defined by `self.interval_seconds`) before repeating the process.
    ///
    /// # Behavior
    /// - If the repository contains metadata (via `latest_metadata()`), it initializes `latest_l2`
    ///   with the `claimed` value from the metadata.
    /// - If the repository has no metadata, it uses `self.initial_claimed` as the starting value.
    /// - The process runs indefinitely, periodically calling `collect` and updating `latest_l2` if new
    ///   claimed metadata is found.
    ///
    /// # Requirements
    /// - The `self.preimage_repository.latest_metadata()` method should return an `Option` containing
    ///   metadata or `None` if no metadata exists.
    /// - The `self.collect(latest_l2)` method should perform the collection operation and return
    ///   an `Option<u64>` indicating a new claimed value, or `None` if no update is available.
    ///
    /// # Note
    /// The loop runs indefinitely. To stop it, external cancellation or shutdown logic should be
    /// implemented, such as utilizing `tokio::task::JoinHandle` or a cancellation token.
    pub async fn start(&self) {
        let mut latest_l2: u64 = match self.preimage_repository.latest_metadata().await {
            Some(metadata) => metadata.claimed,
            None => self.initial_claimed,
        };
        loop {
            if let Some(claimed) = self.collect(latest_l2).await {
                latest_l2 = claimed;
            }
            tokio::time::sleep(time::Duration::from_secs(self.interval_seconds)).await;
        }
    }

    /// Asynchronously collects and processes data starting from a specified `latest_l2` value,
    /// attempting to derive L2 data up to the finalized L2 number obtained from the sync status.
    ///
    /// # Arguments
    ///
    /// * `latest_l2` - The starting point from which L2 derivation and data processing will occur.
    ///
    /// # Steps
    /// 1. The method retrieves the synchronization status of the client and checks if further
    ///    processing is needed by comparing the `latest_l2` with the finalized L2 number. If the
    ///    finalized L2 number is less than or equal to `latest_l2`, no further processing is performed.
    ///
    /// 2. If the sync status is successfully retrieved, the method then fetches the latest L1 head hash
    ///    and raw finality L1 data required for L2 derivation. If unsuccessful, the function exits early.
    ///
    /// 3. It splits the range between `latest_l2` and the finalized L2 number into smaller batches to
    ///    be processed in parallel. The batches are configured based on a maximum distance and concurrency.
    ///
    /// 4. The finalized L1 head hash along with its associated raw finality data is saved to the database.
    ///    Any errors during this step are logged.
    ///
    /// 5. For each batch, a parallel collection process is performed using `parallel_collect`. Any errors
    ///    encountered are logged, and the process stops for the current batch to retry from the latest state.
    ///
    /// # Return Value
    /// * Returns `Some(u64)` with the updated `latest_l2` value if processing is successful.
    /// * Returns `None` if there is an error while fetching the sync status or an issue occurs that halts the process.
    ///
    /// # Errors
    /// * If the synchronization status cannot be retrieved (`sync_status` fetch fails).
    /// * If there is an issue saving the finalized L1 head hash to the database.
    /// * If there is an error during parallel collection in one of the batches.
    ///
    async fn collect(&self, latest_l2: u64) -> Option<u64> {
        let mut latest_l2 = latest_l2;
        // Check sync status
        let sync_status = self.client.sync_status().await;
        let sync_status = match sync_status {
            Ok(sync_status) => {
                info!(
                    "sync status: claimed_l2={}, next_claiming_l2={}, sync_finalized_l1={}",
                    latest_l2, sync_status.finalized_l2.number, sync_status.finalized_l1.number
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

        // Get latest l1 head hash for L2 derivation
        let (l1_head_hash, raw_finality_l1)= self.get_l1_head_hash(&sync_status).await?;

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
        if let Err(e) = self
            .finalized_l1_repository
            .upsert(&l1_head_hash, raw_finality_l1)
            .await
        {
            error!(
                "Failed to save finalized l1 head hash to db l1_head={}, {:?}",
                l1_head_hash, e
            );
        }

        for batch in batches {
            match self.parallel_collect(l1_head_hash, batch.to_vec()).await {
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

    /// Asynchronously retrieves the latest finalized L1 block hash for use in derivation.
    ///
    /// This function communicates with the beacon client to fetch the latest finalized L1 block data.
    /// It ensures the returned block is up-to-date relative to the specified [`SyncStatus`].
    ///
    /// # Parameters
    /// - `sync_status`: A reference to [`SyncStatus`] that includes information about the finalized L1 block number.
    ///
    /// # Returns
    /// - `Option<(B256, String)>`: Returns a tuple containing:
    ///   - The L1 block hash (`B256`) of the finalized L1 block.
    ///   - The raw JSON response (`String`) of the finalized L1 block information from the beacon client.
    ///   - Returns `None` if an error occurs while communicating with the beacon client or if deserialization fails.
    ///
    /// # Behavior
    /// - The function fetches the raw `finality` update from the beacon client.
    /// - It validates that the block is not outdated compared to the `sync_status`.
    /// - In case the retrieved block number is outdated, it waits for a small delay (10 seconds) and retries.
    /// - Logs error or warning messages when issues occur, including:
    ///   - Failure to fetch or deserialize the finality update.
    ///   - Delayed finality L1 blocks.
    /// - If a valid and up-to-date finalized block is found, it returns the L1 block's hash and raw response.
    ///
    /// # Errors
    /// - Logs an error and returns `None` if the beacon client fails to provide a finality update.
    /// - Logs an error and returns `None` if deserialization of the finality update fails.
    ///
    async fn get_l1_head_hash(&self, sync_status: &SyncStatus) -> Option<(B256, String)> {
        let (finality_l1, raw_finality_l1) = loop {
            let raw_finality_l1 = match self
                .beacon_client
                .get_raw_light_client_finality_update()
                .await
            {
                Ok(finality_l1) => finality_l1,
                Err(e) => {
                    error!("Failed to get finality update from beacon client {:?}", e);
                    return None;
                }
            };
            let finality_l1: LightClientFinalityUpdateResponse =
                match serde_json::from_str(&raw_finality_l1) {
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
                continue;
            }
            break (finality_l1, raw_finality_l1);
        };
        let l1_head_hash = finality_l1.data.finalized_header.execution.block_hash;
        info!(
            "l1_head for derivation = {:?}",
            finality_l1.data.finalized_header.execution
        );
        Some((l1_head_hash, raw_finality_l1))
    }

    /// Performs parallel collection of data within a specified range, processes it using multiple
    /// asynchronous tasks, and commits the resulting batch.
    ///
    /// # Parameters
    ///
    /// * `l1_head_hash` - A `B256` hash representing the current L1 chain head.
    /// * `batch` - A vector of tuples, where each tuple represents a range of work (`start`, `end`)
    ///   to process. Each tuple is of the form `(u64, u64)`.
    ///
    /// # Returns
    ///
    /// * `anyhow::Result<Option<u64>>` - If successful, an `Option<u64>` indicating the outcome of the
    ///   operation is returned. If the operation fails, an error is returned.
    ///
    /// # Functionality
    ///
    /// 1. Creates multiple asynchronous tasks to process the range of work defined in the `batch`.
    ///    - Each range element (`start`, `end`) is processed by the `collect` function.
    ///    - The `collect` function performs the desired collection logic with `l2_client` and `config`.
    /// 2. The tasks are executed in parallel using `tokio::spawn`.
    /// 3. Collects the results asynchronously, returning an error if any task fails.
    /// 4. Once all tasks are complete, invokes the `commit_batch` method to commit the collected
    ///    results.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `tokio::spawn` fails to spawn a task.
    /// - A spawned task returns an error.
    /// - `commit_batch` encounters an issue during the commit process.
    ///
    /// ```
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
            results.push(task.await??);
        }

        // Commit
        self.commit_batch(results).await
    }

    /// Commits a batch of preimages to the database and returns the latest claimed block number.
    ///
    /// This method performs the following steps:
    /// - Sorts the input preimages (`successes`) by their claimed block number in ascending order to
    ///   ensure deterministic ordering of preimages.
    /// - Iterates through the sorted preimages and commits each preimage into the database by
    ///   invoking the `upsert` method of the `preimage_repository`.
    /// - Tracks and updates the latest claimed block number during the iteration.
    ///
    /// # Arguments
    /// * `successes` - A vector of tuples where each tuple contains:
    ///     - `PreimageMetadata`: Metadata associated with the preimage.
    ///     - `Vec<u8>`: The preimage data itself.
    ///
    /// # Returns
    /// On success, returns `Ok` with an `Option<u64>`:
    ///   - `Some(u64)`: The latest claimed block number from the preimages.
    ///   - `None`: If the input `successes` vector is empty.
    ///
    /// On failure, an `anyhow::Error` is returned.
    ///
    /// # Errors
    /// This function may return an error in the following cases:
    /// - If the `upsert` operation for the `preimage_repository` fails during the iteration.
    ///
    async fn commit_batch(&self, mut successes: Vec<(PreimageMetadata, Vec<u8>)>) -> anyhow::Result<Option<u64>> {

        // Sort by claimed block number to ensure deterministic order of preimages
        successes.sort_by(|a, b| a.0.claimed.cmp(&b.0.claimed));

        let mut latest_l2 = None;
        for (metadata, preimage) in successes {
            // Commit preimages to db
            let claimed = metadata.claimed;
            self.preimage_repository.upsert(metadata, preimage).await?;
            latest_l2 = Some(claimed);
        }
        Ok(latest_l2)
    }
}

/// Collect preimage for range [start, end]
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

/// Split range [agreed, finalized] into chunks of size chunk.
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
                agreed >= finalized,
                "empty implies agreed >= finalized"
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
