use crate::client::beacon_client::BeaconClient;
use crate::client::l2_client::{L2Client, SyncStatus};
use crate::client::period::compute_period_from_slot;
use crate::data::finalized_l1_repository::{FinalizedL1Data, FinalizedL1Repository};
use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
use crate::derivation::host::single::handler::{Derivation, DerivationConfig, DerivationRequest};
use alloy_primitives::B256;
use axum::async_trait;
use std::sync::Arc;
use tokio::time;
use tracing::{error, info, warn};

/// Threshold for switching from warn to error level logging.
const ERROR_LOG_THRESHOLD: u32 = 70;

/// Logs a message with error level if attempts >= ERROR_LOG_THRESHOLD, otherwise warn level.
macro_rules! warn_or_error {
    ($attempts:expr, $($arg:tt)*) => {
        if $attempts > ERROR_LOG_THRESHOLD {
            error!($($arg)*);
        } else {
            warn!($($arg)*);
        }
    };
}

#[async_trait]
pub trait DerivationDriver: Send + Sync + 'static {
    async fn drive(
        &self,
        config: Arc<DerivationConfig>,
        request: DerivationRequest,
    ) -> anyhow::Result<Vec<u8>>;
}

pub struct RealDerivationDriver;

#[async_trait]
impl DerivationDriver for RealDerivationDriver {
    async fn drive(
        &self,
        config: Arc<DerivationConfig>,
        request: DerivationRequest,
    ) -> anyhow::Result<Vec<u8>> {
        let derivation = Derivation { config, request };
        derivation.start().await
    }
}

/// Builds a batch of block ranges for derivation.
///
/// Each batch element is exactly `distance` in size.
/// Returns an empty vector if there is not enough distance to `finalized_l2`.
fn build_batch(
    latest_l2: u64,
    finalized_l2: u64,
    distance: u64,
    max_concurrency: usize,
) -> Vec<(u64, u64)> {
    let mut batch = Vec::new();
    let mut current = latest_l2;
    for _ in 0..max_concurrency {
        let end = current + distance;
        if end > finalized_l2 {
            break;
        }
        batch.push((current, end));
        current = end;
    }
    batch
}

pub struct PreimageCollector<T, F, L, B, D>
where
    T: PreimageRepository,
    F: FinalizedL1Repository,
    L: L2Client,
    B: BeaconClient,
    D: DerivationDriver,
{
    pub client: Arc<L>,
    pub beacon_client: Arc<B>,
    pub derivation_driver: Arc<D>,
    pub config: Arc<DerivationConfig>,
    pub preimage_repository: Arc<T>,
    pub finalized_l1_repository: Arc<F>,
    pub distance: u64,
    pub max_concurrency: usize,
    pub initial_claimed: u64,
    pub interval_seconds: u64,
}

impl<T, F, L, B, D> PreimageCollector<T, F, L, B, D>
where
    T: PreimageRepository,
    F: FinalizedL1Repository,
    L: L2Client,
    B: BeaconClient,
    D: DerivationDriver,
{
    /// Starts the asynchronous process to continually check and collect claimed metadata.
    ///
    /// This function retrieves the latest claimed metadata from the `preimage_repository`.
    /// It then enters an infinite loop where it performs the following tasks:
    /// 1. Collects metadata using the `collect` method with the `latest_l2` value.
    /// 2. Updates the `latest_l2` value if new metadata is claimed.
    /// 3. Waits for a specified interval (defined by `self.interval_seconds`) before repeating the process.
    ///
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

    /// Collects preimage data based on synchronization status from the L2 chain and persists important finality
    /// information such as the latest finalized L1 head hash.
    ///
    /// 1. **Check Synchronization Status**:
    ///    - Fetches synchronization status from the underlying L2 client.
    ///    - If the L2’s finalized block number (`sync_status.finalized_l2.number`) is less than or equal to
    ///      the provided `latest_l2`, no further processing is performed and `None` is returned.
    ///
    /// 2. **Retrieve L1 Head Hash**:
    ///    - Obtains the hash of the latest L1 head and its raw finality data for L2 derivation purposes.
    ///    - Returns `None` if this operation fails.
    ///
    /// 3. **Batch Collection Preparation**:
    ///    - Prepares a batch of block ranges within the defined constraints (`max_concurrency` and `distance`)
    ///      for processing, starting from the provided `latest_l2` and up to the finalized L2 block in the
    ///      synchronization status.
    ///
    /// 4. **Store Finalized L1 Hash**:
    ///    - Attempts to save the finalized L1 head hash and its associated finality data to the repository.
    ///      Logs errors if the operation fails but continues processing.
    ///
    /// 5. **Parallel Collection**:
    ///    - Executes collection tasks for the prepared block batch in parallel using the computed L1 head hash.
    ///    - Logs and returns `None` on failure, or the latest L2 block number if successful.
    ///
    async fn collect(&self, latest_l2: u64) -> Option<u64> {
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

        let batch = build_batch(
            latest_l2,
            sync_status.finalized_l2.number,
            self.distance,
            self.max_concurrency,
        );

        info!("derivation batch={:?}", batch);

        if batch.is_empty() {
            return None;
        }

        // Get latest l1 head hash and finalized L1 data for L2 derivation
        let (l1_head_hash, finalized_l1_data) =
            self.get_l1_head_and_finalized_data(&sync_status).await?;

        // Save finalized_l1 with light client update
        if let Err(e) = self
            .finalized_l1_repository
            .upsert(&l1_head_hash, finalized_l1_data)
            .await
        {
            error!(
                "Failed to save finalized l1 data to db l1_head={}, {:?}",
                l1_head_hash, e
            );
        }

        self.parallel_collect(l1_head_hash, batch)
            .await
            .unwrap_or_else(|e| {
                error!("Failed to collect preimages {:?}", e);
                None
            })
    }

    /// Asynchronously retrieves the latest finalized L1 block hash and finalized L1 data for derivation.
    ///
    /// This function communicates with the beacon client to fetch the latest finalized L1 block data
    /// and the corresponding light client update for the period.
    ///
    /// - The function fetches the raw `finality` update from the beacon client.
    /// - It validates that the block is not outdated compared to the `sync_status`.
    /// - In case the retrieved block number is outdated, it waits for a small delay (10 seconds) and retries.
    /// - Fetches the light client update for the period corresponding to the finalized slot.
    /// - Logs error or warning messages when issues occur.
    /// - If successful, returns the L1 block's hash and `FinalizedL1Data` containing the finality update,
    ///   light client update, and period.
    ///
    async fn get_l1_head_and_finalized_data(
        &self,
        sync_status: &SyncStatus,
    ) -> Option<(B256, FinalizedL1Data)> {
        let mut attempts_count = 0u32;
        let (finalized_l1_data, finality_update) = loop {
            attempts_count += 1;

            // Get finalized L1 data
            let (finalized_l1_data, finality_update, light_client_update) =
                match self.beacon_client.get_finalized_l1_data().await {
                    Ok(result) => result,
                    Err(e) => {
                        error!("Failed to get finalized L1 data from beacon client {:?}", e);
                        return None;
                    }
                };

            // Check that updates slot <= latest slot to avoid slot order reversal
            // NOTE: This could be true only at the time of period boundary.
            let finalized_slot = finality_update.data.finalized_header.beacon.slot;
            let updates_finalized_slot = light_client_update.data.finalized_header.beacon.slot;
            if updates_finalized_slot > finalized_slot {
                warn_or_error!(
                    attempts_count,
                    "slot mismatch: updates.finalized_slot={} > latest.finalized_slot={}, attempts={}, retrying...",
                    updates_finalized_slot, finalized_slot, attempts_count
                );
                time::sleep(time::Duration::from_secs(10)).await;
                continue;
            }

            // Check period mismatch between signature_slot and finalized_slot
            // NOTE: This could be true only at the time of period boundary.
            let finalized_period = finalized_l1_data.period;
            let signature_slot = finality_update.data.signature_slot;
            let signature_period = compute_period_from_slot(signature_slot);
            if signature_period != finalized_period {
                warn_or_error!(
                    attempts_count,
                    "period mismatch: signature_period={}, finalized_period={}, signature_slot={}, finalized_slot={}, attempts={}, retrying...",
                    signature_period, finalized_period, signature_slot, finalized_slot, attempts_count
                );
                time::sleep(time::Duration::from_secs(10)).await;
                continue;
            }

            // Check block_number against sync_status
            let block_number = finality_update.data.finalized_header.execution.block_number;
            if block_number < sync_status.finalized_l1.number {
                warn_or_error!(
                    attempts_count,
                    "finality_l1 block_number={} delayed (sync_status expects {}), attempts={}, retrying...",
                    block_number, sync_status.finalized_l1.number, attempts_count
                );
                time::sleep(time::Duration::from_secs(10)).await;
                continue;
            }

            // Check sync committee participation
            if finality_update
                .data
                .sync_aggregate
                .is_insufficient_participation()
            {
                warn_or_error!(
                    attempts_count,
                    "insufficient sync committee participation: {:?}, attempts={}, retrying...",
                    finality_update.data.sync_aggregate,
                    attempts_count
                );
                time::sleep(time::Duration::from_secs(10)).await;
                continue;
            }

            break (finalized_l1_data, finality_update);
        };

        info!(
            "l1_head for derivation = {:?}, finalized_slot = {}, finalized_period = {}",
            finality_update.data.finalized_header.execution,
            finality_update.data.finalized_header.beacon.slot,
            finalized_l1_data.period
        );

        let l1_head_hash = finality_update.data.finalized_header.execution.block_hash;
        Some((l1_head_hash, finalized_l1_data))
    }

    /// Performs parallel collection of data within a specified range, processes it using multiple
    /// asynchronous tasks, and commits the resulting batch.
    ///
    /// 1. Creates multiple asynchronous tasks to process the range of work defined in the `batch`.
    ///    - Each range element (`start`, `end`) is processed by the `collect` function.
    ///    - The `collect` function performs the desired collection logic with `l2_client` and `config`.
    /// 2. The tasks are executed in parallel using `tokio::spawn`.
    /// 3. Collects the results asynchronously, returning an error if any task fails.
    /// 4. Once all tasks are complete, invokes the `commit_batch` method to commit the collected
    ///    results.
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
            let derivation_driver = self.derivation_driver.clone();
            tasks.push(tokio::spawn(async move {
                collect(
                    l2_client,
                    config,
                    derivation_driver,
                    l1_head_hash,
                    start,
                    end,
                )
                .await
            }));
        }

        // Wait for all tasks to finish
        let mut results = vec![];
        for task in tasks {
            results.push(task.await??);
        }

        // Commit
        Ok(self.commit_batch(results).await)
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
    async fn commit_batch(&self, mut successes: Vec<(PreimageMetadata, Vec<u8>)>) -> Option<u64> {
        // Sort by claimed block number to ensure deterministic order of preimages
        successes.sort_by(|a, b| a.0.claimed.cmp(&b.0.claimed));

        let mut latest_l2 = None;
        for (metadata, preimage) in successes {
            // Commit preimages to db
            let claimed = metadata.claimed;
            if let Err(e) = self.preimage_repository.upsert(metadata, preimage).await {
                error!("Failed to upsert preimage: {:?}", e);
                return latest_l2;
            }
            latest_l2 = Some(claimed);
        }
        latest_l2
    }
}

/// Collect preimage for range [start, end]
async fn collect<L: L2Client, D: DerivationDriver>(
    client: Arc<L>,
    config: Arc<DerivationConfig>,
    derivation_driver: Arc<D>,
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
    let metadata = PreimageMetadata {
        agreed: start,
        claimed: end,
        l1_head: l1_head_hash,
    };
    let result = derivation_driver.drive(config, request).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::beacon_client::{
        BeaconClient, LightClientFinalityUpdateResponse, LightClientUpdateResponse,
    };
    use crate::client::l2_client::{Block, L2Client, OutputRootAtBlock, SyncStatus};
    use crate::derivation::host::single::config::Config;
    use crate::derivation::host::single::handler::DerivationConfig;
    use alloy_primitives::B256;
    use anyhow::anyhow;
    use axum::async_trait;
    use clap::Parser;
    use kona_genesis::RollupConfig;
    use std::sync::Mutex;

    // Mocks
    struct MockL2Client {
        sync_status: Option<SyncStatus>,
        output_roots: std::collections::HashMap<u64, OutputRootAtBlock>,
    }

    #[async_trait]
    impl L2Client for MockL2Client {
        async fn chain_id(&self) -> anyhow::Result<u64> {
            Ok(10)
        }
        async fn rollup_config(&self) -> anyhow::Result<RollupConfig> {
            Err(anyhow!("unimplemented"))
        }
        async fn sync_status(&self) -> anyhow::Result<SyncStatus> {
            self.sync_status.clone().ok_or(anyhow!("sync status error"))
        }
        async fn output_root_at(&self, number: u64) -> anyhow::Result<OutputRootAtBlock> {
            self.output_roots
                .get(&number)
                .cloned()
                .ok_or(anyhow!("no output root"))
        }
        async fn get_block_by_number(&self, _number: u64) -> anyhow::Result<Block> {
            Err(anyhow!("unimplemented"))
        }
    }

    struct MockBeaconClient {
        finality_update: Option<String>,
    }

    #[async_trait]
    impl BeaconClient for MockBeaconClient {
        async fn get_light_client_finality_update(
            &self,
        ) -> anyhow::Result<(LightClientFinalityUpdateResponse, serde_json::Value)> {
            let json_str = self.finality_update.clone().ok_or(anyhow!("error"))?;
            let value: serde_json::Value = serde_json::from_str(&json_str)?;
            let parsed: LightClientFinalityUpdateResponse = serde_json::from_value(value.clone())?;
            Ok((parsed, value))
        }
        async fn get_light_client_update(
            &self,
            _period: u64,
        ) -> anyhow::Result<(LightClientUpdateResponse, serde_json::Value)> {
            // Return a valid light client update response
            // finalized_slot should be <= finality_update's finalized_slot
            let json_str = r#"{"data":{"finalized_header":{"beacon":{"slot":"100"},"execution":{"block_hash":"0x0000000000000000000000000000000000000000000000000000000000000000","block_number":"95"}},"sync_aggregate":{"sync_committee_bits":"0xffffffff"}}}"#;
            let value: serde_json::Value = serde_json::from_str(json_str)?;
            let parsed: LightClientUpdateResponse = serde_json::from_value(value.clone())?;
            Ok((parsed, value))
        }
        async fn get_genesis(&self) -> anyhow::Result<crate::client::beacon_client::GenesisData> {
            Ok(crate::client::beacon_client::GenesisData { genesis_time: 0 })
        }
    }

    struct MockDerivationDriver {
        calls: Arc<Mutex<Vec<DerivationRequest>>>,
    }

    #[async_trait]
    impl DerivationDriver for MockDerivationDriver {
        async fn drive(
            &self,
            _config: Arc<DerivationConfig>,
            request: DerivationRequest,
        ) -> anyhow::Result<Vec<u8>> {
            self.calls.lock().unwrap().push(request);
            Ok(vec![0x1, 0x2, 0x3])
        }
    }

    struct MockPreimageRepository {
        upserted: Arc<Mutex<Vec<PreimageMetadata>>>,
    }

    #[async_trait]
    impl PreimageRepository for MockPreimageRepository {
        async fn upsert(
            &self,
            metadata: PreimageMetadata,
            _preimage: Vec<u8>,
        ) -> anyhow::Result<()> {
            self.upserted.lock().unwrap().push(metadata);
            Ok(())
        }
        async fn get(&self, _metadata: &PreimageMetadata) -> anyhow::Result<Vec<u8>> {
            Ok(vec![])
        }
        async fn list_metadata(&self, _lt: Option<u64>, _gt: Option<u64>) -> Vec<PreimageMetadata> {
            vec![]
        }
        async fn latest_metadata(&self) -> Option<PreimageMetadata> {
            None
        }
        async fn purge_expired(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockFinalizedL1Repository {
        upserted: Arc<Mutex<Vec<B256>>>,
    }

    #[async_trait]
    impl FinalizedL1Repository for MockFinalizedL1Repository {
        async fn upsert(&self, l1_head_hash: &B256, _data: FinalizedL1Data) -> anyhow::Result<()> {
            self.upserted.lock().unwrap().push(*l1_head_hash);
            Ok(())
        }
        async fn get(&self, _l1_head_hash: &B256) -> anyhow::Result<FinalizedL1Data> {
            Ok(FinalizedL1Data {
                raw_finality_update: serde_json::json!({}),
                raw_light_client_update: serde_json::json!({}),
                period: 0,
            })
        }
        async fn purge_expired(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_collector_collect() {
        let l1_head = B256::repeat_byte(0x11);
        let sync_status = SyncStatus {
            current_l1: dummy_l1(100),
            current_l1_finalized: dummy_l1(90),
            head_l1: dummy_l1(100),
            safe_l1: dummy_l1(95),
            finalized_l1: dummy_l1(90),
            unsafe_l2: dummy_l2(200),
            safe_l2: dummy_l2(190),
            finalized_l2: dummy_l2(150),
            pending_safe_l2: dummy_l2(190),
        };

        // Prepare L2Client mock
        let mut output_roots = std::collections::HashMap::new();
        output_roots.insert(100, dummy_output_root(100)); // agreed
        output_roots.insert(110, dummy_output_root(110)); // target
        output_roots.insert(120, dummy_output_root(120));
        output_roots.insert(130, dummy_output_root(130));
        output_roots.insert(140, dummy_output_root(140));
        output_roots.insert(150, dummy_output_root(150));

        let l2_client = Arc::new(MockL2Client {
            sync_status: Some(sync_status),
            output_roots,
        });

        // Beacon client mock
        // 32 bits all set to 1 (100% participation) for minimal feature
        let sync_committee_bits = "0x".to_string() + &"ff".repeat(4);
        let update_json = serde_json::json!({
             "data": {
                 "finalized_header": {
                     "beacon": {
                         "slot": "100"
                     },
                     "execution": {
                         "block_hash": l1_head,
                         "block_number": "95"
                     }
                 },
                 "sync_aggregate": {
                     "sync_committee_bits": sync_committee_bits
                 },
                 "signature_slot": "105"
             }
        })
        .to_string();
        let beacon_client = Arc::new(MockBeaconClient {
            finality_update: Some(update_json),
        });

        let conf = Config::parse_from([
            "exe",
            "--l1-beacon-address",
            "http://localhost:5052",
            "--l2-node-address",
            "http://localhost:8545",
            "--l2-rollup-address",
            "http://localhost:8545",
            "--preimage-dir",
            "/tmp",
            "--finalized-l1-dir",
            "/tmp",
            "--initial-claimed-l2",
            "0",
        ]); // Partial config ok
        let derivation_config = Arc::new(DerivationConfig {
            config: conf,
            rollup_config: None,
            l2_chain_id: 10,
            l1_chain_config: None,
        });

        let mock_derivations = Arc::new(Mutex::new(vec![]));
        let derivation_driver = Arc::new(MockDerivationDriver {
            calls: mock_derivations.clone(),
        });

        let mock_preimage_repo = Arc::new(MockPreimageRepository {
            upserted: Arc::new(Mutex::new(vec![])),
        });
        let mock_finalized_repo = Arc::new(MockFinalizedL1Repository {
            upserted: Arc::new(Mutex::new(vec![])),
        });

        let collector = PreimageCollector {
            client: l2_client,
            beacon_client,
            derivation_driver,
            config: derivation_config,
            preimage_repository: mock_preimage_repo.clone(),
            finalized_l1_repository: mock_finalized_repo.clone(),
            distance: 10,
            max_concurrency: 2,
            initial_claimed: 0,
            interval_seconds: 1,
        };

        // Start from 100
        let new_head = collector.collect(100).await;

        assert_eq!(new_head, Some(120)); // Should reach limit 120 (100 + 2*10)

        // Verify derivation calls
        let calls = mock_derivations.lock().unwrap();
        // 100->110, 110->120 (2 chunks of size 10)
        assert_eq!(calls.len(), 2);

        // Verify finalized L1 saved
        assert_eq!(mock_finalized_repo.upserted.lock().unwrap().len(), 1);
        assert_eq!(mock_finalized_repo.upserted.lock().unwrap()[0], l1_head);

        // Verify preimages saved
        assert_eq!(mock_preimage_repo.upserted.lock().unwrap().len(), 2);
    }

    fn dummy_l1(number: u64) -> crate::client::l2_client::L1Header {
        crate::client::l2_client::L1Header {
            hash: B256::ZERO,
            number,
            parent_hash: B256::ZERO,
            timestamp: 0,
        }
    }

    fn dummy_l2(number: u64) -> crate::client::l2_client::L2Header {
        crate::client::l2_client::L2Header {
            hash: B256::ZERO,
            number,
            parent_hash: B256::ZERO,
            timestamp: 0,
            l1origin: crate::client::l2_client::L1Origin {
                hash: B256::ZERO,
                number: 0,
            },
            sequence_number: 0,
        }
    }

    fn dummy_output_root(number: u64) -> OutputRootAtBlock {
        OutputRootAtBlock {
            output_root: B256::ZERO,
            block_ref: crate::client::l2_client::L2BlockRef {
                hash: B256::ZERO,
                number,
                l1_origin: crate::client::l2_client::L1Origin {
                    hash: B256::ZERO,
                    number: 0,
                },
            },
        }
    }
    #[tokio::test]
    async fn test_collector_sync_status_reached() {
        let sync_status = SyncStatus {
            current_l1: dummy_l1(100),
            current_l1_finalized: dummy_l1(90),
            head_l1: dummy_l1(100),
            safe_l1: dummy_l1(95),
            finalized_l1: dummy_l1(90),
            unsafe_l2: dummy_l2(200),
            safe_l2: dummy_l2(190),
            finalized_l2: dummy_l2(150),
            pending_safe_l2: dummy_l2(190),
        };

        let l2_client = Arc::new(MockL2Client {
            sync_status: Some(sync_status),
            output_roots: std::collections::HashMap::new(),
        });

        // Beacon client mock (not used if sync status check fails early)
        let beacon_client = Arc::new(MockBeaconClient {
            finality_update: None,
        });

        let conf = Config::parse_from(["exe", "--initial-claimed-l2", "0"]); // Minimal valid config
        let derivation_config = Arc::new(DerivationConfig {
            config: conf,
            rollup_config: None,
            l2_chain_id: 10,
            l1_chain_config: None,
        });

        let derivation_driver = Arc::new(MockDerivationDriver {
            calls: Arc::new(Mutex::new(vec![])),
        });

        let mock_preimage_repo = Arc::new(MockPreimageRepository {
            upserted: Arc::new(Mutex::new(vec![])),
        });
        let mock_finalized_repo = Arc::new(MockFinalizedL1Repository {
            upserted: Arc::new(Mutex::new(vec![])),
        });

        let collector = PreimageCollector {
            client: l2_client,
            beacon_client,
            derivation_driver,
            config: derivation_config,
            preimage_repository: mock_preimage_repo,
            finalized_l1_repository: mock_finalized_repo,
            distance: 10,
            max_concurrency: 2,
            initial_claimed: 0,
            interval_seconds: 1,
        };

        // latest_l2 (150) == finalized_l2 (150)
        let result = collector.collect(150).await;
        assert_eq!(result, None);

        // latest_l2 (160) > finalized_l2 (150)
        let result = collector.collect(160).await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_collector_beacon_client_error() {
        let sync_status = SyncStatus {
            current_l1: dummy_l1(100),
            current_l1_finalized: dummy_l1(90),
            head_l1: dummy_l1(100),
            safe_l1: dummy_l1(95),
            finalized_l1: dummy_l1(90),
            unsafe_l2: dummy_l2(200),
            safe_l2: dummy_l2(190),
            finalized_l2: dummy_l2(150),
            pending_safe_l2: dummy_l2(190),
        };

        let l2_client = Arc::new(MockL2Client {
            sync_status: Some(sync_status),
            output_roots: std::collections::HashMap::new(),
        });

        // Beacon client mock returning error (None)
        let beacon_client = Arc::new(MockBeaconClient {
            finality_update: None,
        });

        let conf = Config::parse_from(["exe", "--initial-claimed-l2", "0"]);
        let derivation_config = Arc::new(DerivationConfig {
            config: conf,
            rollup_config: None,
            l2_chain_id: 10,
            l1_chain_config: None,
        });

        let derivation_driver = Arc::new(MockDerivationDriver {
            calls: Arc::new(Mutex::new(vec![])),
        });

        let mock_preimage_repo = Arc::new(MockPreimageRepository {
            upserted: Arc::new(Mutex::new(vec![])),
        });
        let mock_finalized_repo = Arc::new(MockFinalizedL1Repository {
            upserted: Arc::new(Mutex::new(vec![])),
        });

        let collector = PreimageCollector {
            client: l2_client,
            beacon_client,
            derivation_driver,
            config: derivation_config,
            preimage_repository: mock_preimage_repo,
            finalized_l1_repository: mock_finalized_repo,
            distance: 10,
            max_concurrency: 2,
            initial_claimed: 0,
            interval_seconds: 1,
        };

        // Should return None due to beacon client error
        let result = collector.collect(100).await;
        assert_eq!(result, None);
    }

    struct MockDerivationDriverError;
    #[async_trait]
    impl DerivationDriver for MockDerivationDriverError {
        async fn drive(
            &self,
            _config: Arc<DerivationConfig>,
            _request: DerivationRequest,
        ) -> anyhow::Result<Vec<u8>> {
            Err(anyhow!("derivation error"))
        }
    }

    #[tokio::test]
    async fn test_collector_derivation_failure() {
        let l1_head = B256::repeat_byte(0x11);
        let sync_status = SyncStatus {
            current_l1: dummy_l1(100),
            current_l1_finalized: dummy_l1(90),
            head_l1: dummy_l1(100),
            safe_l1: dummy_l1(95),
            finalized_l1: dummy_l1(90),
            unsafe_l2: dummy_l2(200),
            safe_l2: dummy_l2(190),
            finalized_l2: dummy_l2(150),
            pending_safe_l2: dummy_l2(190),
        };

        let mut output_roots = std::collections::HashMap::new();
        output_roots.insert(100, dummy_output_root(100));
        output_roots.insert(110, dummy_output_root(110));

        let l2_client = Arc::new(MockL2Client {
            sync_status: Some(sync_status),
            output_roots,
        });

        // 32 bits all set to 1 (100% participation) for minimal feature
        let sync_committee_bits = "0x".to_string() + &"ff".repeat(4);
        let update_json = serde_json::json!({
             "data": {
                 "finalized_header": {
                     "beacon": {
                         "slot": "100"
                     },
                     "execution": {
                         "block_hash": l1_head,
                         "block_number": "95"
                     }
                 },
                 "sync_aggregate": {
                     "sync_committee_bits": sync_committee_bits
                 },
                 "signature_slot": "105"
             }
        })
        .to_string();
        let beacon_client = Arc::new(MockBeaconClient {
            finality_update: Some(update_json),
        });

        let conf = Config::parse_from(["exe", "--initial-claimed-l2", "0"]);
        let derivation_config = Arc::new(DerivationConfig {
            config: conf,
            rollup_config: None,
            l2_chain_id: 10,
            l1_chain_config: None,
        });

        let derivation_driver = Arc::new(MockDerivationDriverError); // Fails derivation

        let mock_preimage_repo = Arc::new(MockPreimageRepository {
            upserted: Arc::new(Mutex::new(vec![])),
        });
        let mock_finalized_repo = Arc::new(MockFinalizedL1Repository {
            upserted: Arc::new(Mutex::new(vec![])),
        });

        let collector = PreimageCollector {
            client: l2_client,
            beacon_client,
            derivation_driver,
            config: derivation_config,
            preimage_repository: mock_preimage_repo.clone(),
            finalized_l1_repository: mock_finalized_repo,
            distance: 10,
            max_concurrency: 2,
            initial_claimed: 0,
            interval_seconds: 1,
        };

        // Should return None because derivation failed (triggering retry)
        let result = collector.collect(100).await;
        assert_eq!(result, None);

        // Verify preimages were NOT saved
        assert_eq!(mock_preimage_repo.upserted.lock().unwrap().len(), 0);
    }

    struct MockPreimageRepositoryPartialError {
        upserted: Arc<Mutex<Vec<PreimageMetadata>>>,
        fail_index: usize,
    }

    #[async_trait]
    impl PreimageRepository for MockPreimageRepositoryPartialError {
        async fn upsert(
            &self,
            metadata: PreimageMetadata,
            _preimage: Vec<u8>,
        ) -> anyhow::Result<()> {
            let mut upserted = self.upserted.lock().unwrap();
            if upserted.len() == self.fail_index {
                return Err(anyhow!("partial error"));
            }
            upserted.push(metadata);
            Ok(())
        }
        async fn get(&self, _metadata: &PreimageMetadata) -> anyhow::Result<Vec<u8>> {
            Ok(vec![])
        }
        async fn list_metadata(&self, _lt: Option<u64>, _gt: Option<u64>) -> Vec<PreimageMetadata> {
            vec![]
        }
        async fn latest_metadata(&self) -> Option<PreimageMetadata> {
            None
        }
        async fn purge_expired(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_collector_commit_batch_partial_failure() {
        let l1_head = B256::repeat_byte(0x11);
        let sync_status = SyncStatus {
            current_l1: dummy_l1(100),
            current_l1_finalized: dummy_l1(90),
            head_l1: dummy_l1(100),
            safe_l1: dummy_l1(95),
            finalized_l1: dummy_l1(90),
            unsafe_l2: dummy_l2(200),
            safe_l2: dummy_l2(190),
            finalized_l2: dummy_l2(150),
            pending_safe_l2: dummy_l2(190),
        };

        let mut output_roots = std::collections::HashMap::new();
        output_roots.insert(100, dummy_output_root(100));
        output_roots.insert(110, dummy_output_root(110));
        output_roots.insert(120, dummy_output_root(120));

        let l2_client = Arc::new(MockL2Client {
            sync_status: Some(sync_status),
            output_roots,
        });

        // 32 bits all set to 1 (100% participation) for minimal feature
        let sync_committee_bits = "0x".to_string() + &"ff".repeat(4);
        let update_json = serde_json::json!({
             "data": {
                 "finalized_header": {
                     "beacon": {
                         "slot": "100"
                     },
                     "execution": {
                         "block_hash": l1_head,
                         "block_number": "95"
                     }
                 },
                 "sync_aggregate": {
                     "sync_committee_bits": sync_committee_bits
                 },
                 "signature_slot": "105"
             }
        })
        .to_string();
        let beacon_client = Arc::new(MockBeaconClient {
            finality_update: Some(update_json),
        });

        let conf = Config::parse_from(["exe", "--initial-claimed-l2", "0"]);
        let derivation_config = Arc::new(DerivationConfig {
            config: conf,
            rollup_config: None,
            l2_chain_id: 10,
            l1_chain_config: None,
        });

        let derivation_driver = Arc::new(MockDerivationDriver {
            calls: Arc::new(Mutex::new(vec![])),
        });

        // Fail on the 2nd upsert (index 1)
        let mock_preimage_repo = Arc::new(MockPreimageRepositoryPartialError {
            upserted: Arc::new(Mutex::new(vec![])),
            fail_index: 1,
        });
        let mock_finalized_repo = Arc::new(MockFinalizedL1Repository {
            upserted: Arc::new(Mutex::new(vec![])),
        });

        let collector = PreimageCollector {
            client: l2_client,
            beacon_client,
            derivation_driver,
            config: derivation_config,
            preimage_repository: mock_preimage_repo.clone(),
            finalized_l1_repository: mock_finalized_repo,
            distance: 10,
            max_concurrency: 2,
            initial_claimed: 0,
            interval_seconds: 1,
        };

        // Collect from 100.
        // Batch will be [100->110, 110->120].
        // 1st upsert (110) succeeds.
        // 2nd upsert (120) fails.
        // Should return Some(110).
        let result = collector.collect(100).await;

        assert_eq!(result, Some(110));

        // Verify only 1 preimage saved
        assert_eq!(mock_preimage_repo.upserted.lock().unwrap().len(), 1);
        assert_eq!(mock_preimage_repo.upserted.lock().unwrap()[0].claimed, 110);
    }

    #[test]
    fn test_build_batch_full_batches() {
        // latest_l2=100, finalized_l2=150, distance=10, max_concurrency=3
        // Expected: [(100,110), (110,120), (120,130)]
        let batch = build_batch(100, 150, 10, 3);
        assert_eq!(batch, vec![(100, 110), (110, 120), (120, 130)]);
    }

    #[test]
    fn test_build_batch_limited_by_finalized() {
        // latest_l2=100, finalized_l2=125, distance=10, max_concurrency=5
        // Expected: [(100,110), (110,120)] (120+10=130 > 125, so stop)
        let batch = build_batch(100, 125, 10, 5);
        assert_eq!(batch, vec![(100, 110), (110, 120)]);
    }

    #[test]
    fn test_build_batch_limited_by_concurrency() {
        // latest_l2=100, finalized_l2=200, distance=10, max_concurrency=2
        // Expected: [(100,110), (110,120)] (limited by max_concurrency)
        let batch = build_batch(100, 200, 10, 2);
        assert_eq!(batch, vec![(100, 110), (110, 120)]);
    }

    #[test]
    fn test_build_batch_empty_not_enough_distance() {
        // latest_l2=100, finalized_l2=105, distance=10, max_concurrency=3
        // Expected: [] (100+10=110 > 105)
        let batch = build_batch(100, 105, 10, 3);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_build_batch_empty_same_block() {
        // latest_l2=100, finalized_l2=100, distance=10, max_concurrency=3
        // Expected: []
        let batch = build_batch(100, 100, 10, 3);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_build_batch_exact_boundary() {
        // latest_l2=100, finalized_l2=120, distance=10, max_concurrency=3
        // Expected: [(100,110), (110,120)] (110+10=120 == 120, so included)
        let batch = build_batch(100, 120, 10, 3);
        assert_eq!(batch, vec![(100, 110), (110, 120)]);
    }

    // Mock that returns different responses on each call for testing retry logic
    struct MockBeaconClientWithRetry {
        finality_updates: Arc<Mutex<Vec<String>>>,
        light_client_updates: Arc<Mutex<Vec<String>>>,
        call_count: Arc<Mutex<usize>>,
    }

    impl MockBeaconClientWithRetry {
        fn new(finality_updates: Vec<String>, light_client_updates: Vec<String>) -> Self {
            Self {
                finality_updates: Arc::new(Mutex::new(finality_updates)),
                light_client_updates: Arc::new(Mutex::new(light_client_updates)),
                call_count: Arc::new(Mutex::new(0)),
            }
        }
    }

    #[async_trait]
    impl BeaconClient for MockBeaconClientWithRetry {
        async fn get_light_client_finality_update(
            &self,
        ) -> anyhow::Result<(LightClientFinalityUpdateResponse, serde_json::Value)> {
            let mut count = self.call_count.lock().unwrap();
            let updates = self.finality_updates.lock().unwrap();
            let idx = (*count).min(updates.len() - 1);
            let json_str = updates[idx].clone();
            *count += 1;
            drop(count);
            drop(updates);

            let value: serde_json::Value = serde_json::from_str(&json_str)?;
            let parsed: LightClientFinalityUpdateResponse = serde_json::from_value(value.clone())?;
            Ok((parsed, value))
        }

        async fn get_light_client_update(
            &self,
            _period: u64,
        ) -> anyhow::Result<(LightClientUpdateResponse, serde_json::Value)> {
            let count = self.call_count.lock().unwrap();
            let updates = self.light_client_updates.lock().unwrap();
            // Use count - 1 because get_light_client_finality_update is called first
            let idx = ((*count).saturating_sub(1)).min(updates.len() - 1);
            let json_str = updates[idx].clone();
            drop(count);
            drop(updates);

            let value: serde_json::Value = serde_json::from_str(&json_str)?;
            let parsed: LightClientUpdateResponse = serde_json::from_value(value.clone())?;
            Ok((parsed, value))
        }

        async fn get_genesis(&self) -> anyhow::Result<crate::client::beacon_client::GenesisData> {
            Ok(crate::client::beacon_client::GenesisData { genesis_time: 0 })
        }
    }

    fn make_finality_update_json(
        l1_head: B256,
        slot: u64,
        block_number: u64,
        signature_slot: u64,
        sync_committee_bits: &str,
    ) -> String {
        serde_json::json!({
            "data": {
                "finalized_header": {
                    "beacon": { "slot": slot.to_string() },
                    "execution": {
                        "block_hash": l1_head,
                        "block_number": block_number.to_string()
                    }
                },
                "sync_aggregate": { "sync_committee_bits": sync_committee_bits },
                "signature_slot": signature_slot.to_string()
            }
        })
        .to_string()
    }

    fn make_light_client_update_json(slot: u64) -> String {
        serde_json::json!({
            "data": {
                "finalized_header": {
                    "beacon": { "slot": slot.to_string() },
                    "execution": {
                        "block_hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                        "block_number": "95"
                    }
                },
                "sync_aggregate": { "sync_committee_bits": "0xffffffff" }
            }
        })
        .to_string()
    }

    // Helper to create a collector for retry tests
    async fn create_collector_for_retry_test(
        beacon_client: Arc<MockBeaconClientWithRetry>,
        finalized_l1_number: u64,
    ) -> PreimageCollector<
        MockPreimageRepository,
        MockFinalizedL1Repository,
        MockL2Client,
        MockBeaconClientWithRetry,
        MockDerivationDriver,
    > {
        let sync_status = SyncStatus {
            current_l1: dummy_l1(100),
            current_l1_finalized: dummy_l1(finalized_l1_number),
            head_l1: dummy_l1(100),
            safe_l1: dummy_l1(95),
            finalized_l1: dummy_l1(finalized_l1_number),
            unsafe_l2: dummy_l2(200),
            safe_l2: dummy_l2(190),
            finalized_l2: dummy_l2(150),
            pending_safe_l2: dummy_l2(190),
        };

        let mut output_roots = std::collections::HashMap::new();
        output_roots.insert(100, dummy_output_root(100));
        output_roots.insert(110, dummy_output_root(110));

        let l2_client = Arc::new(MockL2Client {
            sync_status: Some(sync_status),
            output_roots,
        });

        let conf = Config::parse_from(["exe", "--initial-claimed-l2", "0"]);
        let derivation_config = Arc::new(DerivationConfig {
            config: conf,
            rollup_config: None,
            l2_chain_id: 10,
            l1_chain_config: None,
        });

        let derivation_driver = Arc::new(MockDerivationDriver {
            calls: Arc::new(Mutex::new(vec![])),
        });

        let mock_preimage_repo = Arc::new(MockPreimageRepository {
            upserted: Arc::new(Mutex::new(vec![])),
        });
        let mock_finalized_repo = Arc::new(MockFinalizedL1Repository {
            upserted: Arc::new(Mutex::new(vec![])),
        });

        PreimageCollector {
            client: l2_client,
            beacon_client,
            derivation_driver,
            config: derivation_config,
            preimage_repository: mock_preimage_repo,
            finalized_l1_repository: mock_finalized_repo,
            distance: 10,
            max_concurrency: 2,
            initial_claimed: 0,
            interval_seconds: 1,
        }
    }

    #[tokio::test]
    async fn test_get_l1_head_retry_on_slot_mismatch() {
        // Test: updates_finalized_slot > finalized_slot triggers retry
        // First call: light_client_update has slot 200, finality_update has slot 100 -> mismatch
        // Second call: both have slot 100 -> success
        let l1_head = B256::repeat_byte(0x11);
        let full_participation = "0x".to_string() + &"ff".repeat(4);

        let finality_updates = vec![
            make_finality_update_json(l1_head, 100, 95, 105, &full_participation),
            make_finality_update_json(l1_head, 100, 95, 105, &full_participation),
        ];
        let light_client_updates = vec![
            make_light_client_update_json(200), // slot 200 > 100, triggers retry
            make_light_client_update_json(100), // slot 100 <= 100, success
        ];

        let beacon_client = Arc::new(MockBeaconClientWithRetry::new(
            finality_updates,
            light_client_updates,
        ));

        let collector = create_collector_for_retry_test(beacon_client.clone(), 90).await;
        let sync_status = collector.client.sync_status().await.unwrap();

        let result = collector.get_l1_head_and_finalized_data(&sync_status).await;

        assert!(result.is_some());
        // Verify retry happened (call_count should be 2)
        assert_eq!(*beacon_client.call_count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_get_l1_head_retry_on_period_mismatch() {
        // Test: signature_period != finalized_period triggers retry
        // Period boundary: slot 8192 is period 1, slot 8191 is period 0
        // First call: finalized_slot in period 0, signature_slot in period 1 -> mismatch
        // Second call: both in same period -> success
        let l1_head = B256::repeat_byte(0x11);
        let full_participation = "0x".to_string() + &"ff".repeat(4);

        let finality_updates = vec![
            // finalized_slot=8191 (period 0), signature_slot=8192 (period 1) -> mismatch
            make_finality_update_json(l1_head, 8191, 95, 8192, &full_participation),
            // finalized_slot=8192 (period 1), signature_slot=8193 (period 1) -> same period
            make_finality_update_json(l1_head, 8192, 95, 8193, &full_participation),
        ];
        let light_client_updates = vec![
            make_light_client_update_json(8191),
            make_light_client_update_json(8192),
        ];

        let beacon_client = Arc::new(MockBeaconClientWithRetry::new(
            finality_updates,
            light_client_updates,
        ));

        let collector = create_collector_for_retry_test(beacon_client.clone(), 90).await;
        let sync_status = collector.client.sync_status().await.unwrap();

        let result = collector.get_l1_head_and_finalized_data(&sync_status).await;

        assert!(result.is_some());
        assert_eq!(*beacon_client.call_count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_get_l1_head_retry_on_block_number_delayed() {
        // Test: block_number < sync_status.finalized_l1.number triggers retry
        // First call: block_number=80, sync_status expects 90 -> delayed
        // Second call: block_number=95 >= 90 -> success
        let l1_head = B256::repeat_byte(0x11);
        let full_participation = "0x".to_string() + &"ff".repeat(4);

        let finality_updates = vec![
            make_finality_update_json(l1_head, 100, 80, 105, &full_participation), // block_number=80 < 90
            make_finality_update_json(l1_head, 100, 95, 105, &full_participation), // block_number=95 >= 90
        ];
        let light_client_updates = vec![
            make_light_client_update_json(100),
            make_light_client_update_json(100),
        ];

        let beacon_client = Arc::new(MockBeaconClientWithRetry::new(
            finality_updates,
            light_client_updates,
        ));

        // sync_status.finalized_l1.number = 90
        let collector = create_collector_for_retry_test(beacon_client.clone(), 90).await;
        let sync_status = collector.client.sync_status().await.unwrap();

        let result = collector.get_l1_head_and_finalized_data(&sync_status).await;

        assert!(result.is_some());
        assert_eq!(*beacon_client.call_count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_get_l1_head_retry_on_insufficient_participation() {
        // Test: insufficient sync committee participation triggers retry
        // First call: only 20/32 participants (62.5% < 66.67%) -> insufficient
        // Second call: 32/32 participants (100%) -> sufficient
        let l1_head = B256::repeat_byte(0x11);
        // 32 bits set
        let full_participation = "0x".to_string() + &"ff".repeat(4);
        // 20 bits set: 0xfffff (20 bits) = 5 bytes of ff would be 40 bits, we need 20 bits
        // 20 bits = 0x000fffff -> but we need hex for Bitvector<32>
        // Actually for minimal feature (32 bits = 4 bytes):
        // 20 bits set = 0x000fffff (but only first 20 bits set)
        // Let's use 0x0000ffff (16 bits) which is clearly < 2/3 of 32
        let insufficient_participation = "0x0000ffff"; // 16/32 = 50% < 66.67%

        let finality_updates = vec![
            make_finality_update_json(l1_head, 100, 95, 105, insufficient_participation),
            make_finality_update_json(l1_head, 100, 95, 105, &full_participation),
        ];
        let light_client_updates = vec![
            make_light_client_update_json(100),
            make_light_client_update_json(100),
        ];

        let beacon_client = Arc::new(MockBeaconClientWithRetry::new(
            finality_updates,
            light_client_updates,
        ));

        let collector = create_collector_for_retry_test(beacon_client.clone(), 90).await;
        let sync_status = collector.client.sync_status().await.unwrap();

        let result = collector.get_l1_head_and_finalized_data(&sync_status).await;

        assert!(result.is_some());
        assert_eq!(*beacon_client.call_count.lock().unwrap(), 2);
    }
}
