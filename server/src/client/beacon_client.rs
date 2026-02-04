use alloy_primitives::B256;
use axum::async_trait;
use reqwest::Response;
use ssz_rs::Bitvector;
use tracing::warn;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientFinalityUpdateResponse {
    pub data: LightClientFinalityUpdate,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientFinalityUpdate {
    pub finalized_header: LightClientHeader,
    pub sync_aggregate: SyncAggregate,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientHeader {
    pub execution: ExecutionPayloadHeader,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExecutionPayloadHeader {
    pub block_hash: B256,
    #[serde(deserialize_with = "deserialize_u64_from_str")]
    pub block_number: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SyncAggregate {
    #[cfg(feature = "minimal")]
    pub sync_committee_bits: Bitvector<32>,
    #[cfg(not(feature = "minimal"))]
    pub sync_committee_bits: Bitvector<512>,
}

impl SyncAggregate {
    pub fn is_sufficient_participation(&self) -> bool {
        let participants = self.sync_committee_bits.count_ones() as u64;
        let validators = self.sync_committee_bits.len() as u64;
        const TRUST_LEVEL_NUMERATOR: u64 = 2;
        const TRUST_LEVEL_DENOMINATOR: u64 = 3;
        let insufficient =
            participants * TRUST_LEVEL_DENOMINATOR < validators * TRUST_LEVEL_NUMERATOR;
        if insufficient {
            warn!(
                "Insufficient sync committee participation rate: participants={} validators={}",
                participants, validators
            );
            return false;
        }
        true
    }
}

// serde helper to allow numbers encoded as strings
fn deserialize_u64_from_str<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct U64StringOrNumber;

    impl<'de> serde::de::Visitor<'de> for U64StringOrNumber {
        type Value = u64;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "a u64 as number or string")
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
            Ok(v)
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if v < 0 {
                return Err(E::custom("negative value for u64"));
            }
            Ok(v as u64)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse::<u64>()
                .map_err(|e| E::custom(format!("invalid u64 string: {e}")))
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            self.visit_str(&v)
        }
    }

    deserializer.deserialize_any(U64StringOrNumber)
}

#[async_trait]
pub trait BeaconClient: Send + Sync + 'static {
    async fn get_raw_light_client_finality_update(&self) -> anyhow::Result<String>;
}

#[derive(Debug, Clone)]
pub struct HttpBeaconClient {
    beacon_addr: String,
    client: reqwest::Client,
}

impl HttpBeaconClient {
    pub fn new(beacon_addr: String, timeout: std::time::Duration) -> Self {
        Self {
            beacon_addr,
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    async fn check_response(&self, response: Response) -> anyhow::Result<Response> {
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(anyhow::anyhow!(
                "Request failed with status: {} body={:?}",
                response.status(),
                response.text().await
            ))
        }
    }
}

#[async_trait]
impl BeaconClient for HttpBeaconClient {
    async fn get_raw_light_client_finality_update(&self) -> anyhow::Result<String> {
        let response = self
            .client
            .get(format!(
                "{}/eth/v1/beacon/light_client/finality_update",
                self.beacon_addr
            ))
            .send()
            .await?;
        let response = self.check_response(response).await?;
        response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get finality update: {e:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_sync_aggregate_with_participation(num_participants: usize) -> SyncAggregate {
        #[cfg(feature = "minimal")]
        const SIZE: usize = 32;
        #[cfg(not(feature = "minimal"))]
        const SIZE: usize = 512;

        let mut bits = Bitvector::<SIZE>::default();
        for i in 0..num_participants.min(SIZE) {
            bits.set(i, true);
        }
        SyncAggregate {
            sync_committee_bits: bits,
        }
    }

    #[test]
    fn test_is_sufficient_participation_all_participants() {
        // 100% participation should be sufficient
        #[cfg(feature = "minimal")]
        let agg = create_sync_aggregate_with_participation(32);
        #[cfg(not(feature = "minimal"))]
        let agg = create_sync_aggregate_with_participation(512);

        assert!(agg.is_sufficient_participation());
    }

    #[test]
    fn test_is_sufficient_participation_two_thirds() {
        // Exactly 2/3 participation should be sufficient (participants >= validators * 2/3)
        #[cfg(feature = "minimal")]
        let agg = create_sync_aggregate_with_participation(22); // 22/32 = 68.75% >= 66.67%
        #[cfg(not(feature = "minimal"))]
        let agg = create_sync_aggregate_with_participation(342); // 342/512 = 66.80% >= 66.67%

        assert!(agg.is_sufficient_participation());
    }

    #[test]
    fn test_is_sufficient_participation_below_threshold() {
        // Less than 2/3 participation should be insufficient
        #[cfg(feature = "minimal")]
        let agg = create_sync_aggregate_with_participation(20); // 20/32 = 62.5% < 66.67%
        #[cfg(not(feature = "minimal"))]
        let agg = create_sync_aggregate_with_participation(340); // 340/512 = 66.41% < 66.67%

        assert!(!agg.is_sufficient_participation());
    }

    #[test]
    fn test_is_sufficient_participation_no_participants() {
        // 0 participation should be insufficient
        let agg = create_sync_aggregate_with_participation(0);
        assert!(!agg.is_sufficient_participation());
    }

    #[test]
    fn test_is_sufficient_participation_boundary() {
        // Test the exact boundary: participants * 3 >= validators * 2
        // For 512 validators: 512 * 2 / 3 = 341.33..., so need 342 participants
        // For 32 validators: 32 * 2 / 3 = 21.33..., so need 22 participants
        #[cfg(feature = "minimal")]
        {
            let agg_pass = create_sync_aggregate_with_participation(22);
            let agg_fail = create_sync_aggregate_with_participation(21);
            assert!(agg_pass.is_sufficient_participation());
            assert!(!agg_fail.is_sufficient_participation());
        }
        #[cfg(not(feature = "minimal"))]
        {
            let agg_pass = create_sync_aggregate_with_participation(342);
            let agg_fail = create_sync_aggregate_with_participation(341);
            assert!(agg_pass.is_sufficient_participation());
            assert!(!agg_fail.is_sufficient_participation());
        }
    }
}
