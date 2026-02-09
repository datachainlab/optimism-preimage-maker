use crate::data::finalized_l1_repository::FinalizedL1Repository;
use crate::data::preimage_repository::PreimageRepository;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

pub struct PreimagePurger<T: PreimageRepository, F: FinalizedL1Repository> {
    pub preimage_repository: Arc<T>,
    pub finalized_l1_repository: Arc<F>,
    pub interval_seconds: u64,
}

impl<T: PreimageRepository, F: FinalizedL1Repository> PreimagePurger<T, F> {
    pub async fn start(&self) {
        loop {
            self.run_once().await;
            time::sleep(Duration::from_secs(self.interval_seconds)).await;
        }
    }

    pub async fn run_once(&self) {
        tracing::info!("start: purge expired preimages");
        if let Err(e) = self.preimage_repository.purge_expired().await {
            tracing::error!("failed to purge expired preimages: {:?}", e);
        }
        if let Err(e) = self.finalized_l1_repository.purge_expired().await {
            tracing::error!("failed to purge expired finalized l1 heads: {:?}", e);
        }
        tracing::info!("end: purge expired preimages");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::finalized_l1_repository::FinalizedL1Data;
    use crate::data::preimage_repository::PreimageMetadata;
    use alloy_primitives::B256;
    use axum::async_trait;
    use serde_json;
    use std::sync::{Arc, Mutex};

    struct MockPreimageRepository {
        pub purged: Arc<Mutex<bool>>,
    }
    #[async_trait]
    impl PreimageRepository for MockPreimageRepository {
        async fn upsert(
            &self,
            _metadata: PreimageMetadata,
            _preimage: Vec<u8>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn get(&self, _metadata: &PreimageMetadata) -> anyhow::Result<Vec<u8>> {
            Ok(vec![])
        }
        async fn list_metadata(
            &self,
            _lt_claimed: Option<u64>,
            _gt_claimed: Option<u64>,
        ) -> Vec<PreimageMetadata> {
            vec![]
        }
        async fn latest_metadata(&self) -> Option<PreimageMetadata> {
            None
        }
        async fn purge_expired(&self) -> anyhow::Result<()> {
            *self.purged.lock().unwrap() = true;
            Ok(())
        }
    }

    struct MockFinalizedL1Repository {
        pub purged: Arc<Mutex<bool>>,
    }
    #[async_trait]
    impl FinalizedL1Repository for MockFinalizedL1Repository {
        async fn upsert(&self, _l1_head_hash: &B256, _data: FinalizedL1Data) -> anyhow::Result<()> {
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
            *self.purged.lock().unwrap() = true;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_purger_run_once() {
        let p_purged = Arc::new(Mutex::new(false));
        let l1_purged = Arc::new(Mutex::new(false));

        let p_repo = MockPreimageRepository {
            purged: p_purged.clone(),
        };
        let l1_repo = MockFinalizedL1Repository {
            purged: l1_purged.clone(),
        };

        let purger = PreimagePurger {
            preimage_repository: Arc::new(p_repo),
            finalized_l1_repository: Arc::new(l1_repo),
            interval_seconds: 1,
        };

        purger.run_once().await;

        assert!(*p_purged.lock().unwrap());
        assert!(*l1_purged.lock().unwrap());
    }
}
