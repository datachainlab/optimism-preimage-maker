use alloy_primitives::B256;
use alloy_primitives::hex::FromHex;
use axum::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreimageMetadata {
    pub agreed: u64,
    pub claimed: u64,
    pub l1_head: B256
}

impl TryFrom<&str> for PreimageMetadata {
    type Error = anyhow::Error;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        let split = name.split("_").collect::<Vec<&str>>();
        if split.len() != 3 {
            return anyhow::bail!("invalid preimage name: {}", name);
        }
        let agreed_l2 : u64 = split[0].parse()?;
        let claimed_l2: u64 = split[1].parse()?;
        let l1_head_hash = B256::from_hex(split[2])?;
        Ok(PreimageMetadata {
            agreed: agreed_l2,
            claimed: claimed_l2,
            l1_head: l1_head_hash
        })
    }
}

#[async_trait]
pub trait PreimageRepository : Send + Sync {
    async fn upsert(&self, metadata: PreimageMetadata, preimage: Vec<u8>) -> anyhow::Result<()>;

    async fn get(&self, metadata: &PreimageMetadata) -> anyhow::Result<Vec<u8>>;

    async fn list_metadata(&self, gt_claimed: Option<u64>) -> Vec<PreimageMetadata>;

    async fn latest_metadata(&self) -> Option<PreimageMetadata>;
}