use alloy_primitives::hex::FromHex;
use alloy_primitives::B256;
use axum::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreimageMetadata {
    pub agreed: u64,
    pub claimed: u64,
    pub l1_head: B256,
}

impl TryFrom<&str> for PreimageMetadata {
    type Error = anyhow::Error;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        let split = name.split("_").collect::<Vec<&str>>();
        if split.len() != 3 {
            return Err(anyhow::anyhow!("invalid preimage name: {name}"));
        }
        let agreed_l2: u64 = split[0].parse()?;
        let claimed_l2: u64 = split[1].parse()?;
        let l1_head_hash = B256::from_hex(split[2])?;
        Ok(PreimageMetadata {
            agreed: agreed_l2,
            claimed: claimed_l2,
            l1_head: l1_head_hash,
        })
    }
}

#[async_trait]
pub trait PreimageRepository: Send + Sync {
    async fn upsert(&self, metadata: PreimageMetadata, preimage: Vec<u8>) -> anyhow::Result<()>;

    async fn get(&self, metadata: &PreimageMetadata) -> anyhow::Result<Vec<u8>>;

    async fn list_metadata(
        &self,
        lt_claimed: Option<u64>,
        gt_claimed: Option<u64>,
    ) -> Vec<PreimageMetadata>;

    async fn latest_metadata(&self) -> Option<PreimageMetadata>;

    async fn purge_expired(&self) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::PreimageMetadata;
    use alloy_primitives::B256;
    // for hex::encode
    use serde_json;

    #[test]
    fn test_parse_invalid_component_count() {
        let bad = "1_2"; // only two parts
        let err = PreimageMetadata::try_from(bad).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("invalid preimage name"));
    }

    #[test]
    fn test_parse_invalid_numbers() {
        // non-numeric agreed
        let h = B256::from([0x33u8; 32]);
        let s1 = format!("x_2_{h}");
        assert!(PreimageMetadata::try_from(s1.as_str()).is_err());

        // non-numeric claimed
        let s2 = format!("1_y_{h}");
        assert!(PreimageMetadata::try_from(s2.as_str()).is_err());
    }

    #[test]
    fn test_parse_invalid_hash() {
        let bad = "1_2_nothex";
        assert!(PreimageMetadata::try_from(bad).is_err());
    }

    #[test]
    fn test_serde_roundtrip() {
        let m = PreimageMetadata {
            agreed: 123,
            claimed: 456,
            l1_head: B256::from([0x44u8; 32]),
        };
        let json = serde_json::to_string(&m).expect("serialize");
        let back: PreimageMetadata = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, m);
    }

    #[test]
    fn test_filename_roundtrip() {
        let m = PreimageMetadata {
            agreed: 7,
            claimed: 8,
            l1_head: B256::from([0x55u8; 32]),
        };
        let name = format!("{}_{}_{}", m.agreed, m.claimed, m.l1_head);
        let parsed = PreimageMetadata::try_from(name.as_str()).expect("parse ok");
        assert_eq!(parsed, m);
    }
}
