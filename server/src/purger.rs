use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use crate::data::finalized_l1_repository::FinalizedL1Repository;
use crate::data::preimage_repository::PreimageRepository;

pub struct PreimagePurger<T: PreimageRepository, F: FinalizedL1Repository> {
    pub preimage_repository: Arc<T>,
    pub finalized_l1_repository: Arc<F>,
    pub interval_seconds: u64,
}

impl<T: PreimageRepository, F: FinalizedL1Repository> PreimagePurger<T, F> {
    pub async fn start(&self) {
        loop {
            tracing::info!("start: purge expired preimages");
            if let Err(e) = self.preimage_repository.purge_expired().await {
                tracing::error!("failed to purge expired preimages: {:?}", e);
            }
            if let Err(e) = self.finalized_l1_repository.purge_expired().await {
                tracing::error!("failed to purge expired finalized l1 heads: {:?}", e);
            }
            tracing::info!("end: purge expired preimages");
            time::sleep(Duration::from_secs(self.interval_seconds)).await;
        }
    }
}