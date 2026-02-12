use crate::client::period::compute_period_from_slot;
use crate::data::finalized_l1_repository::FinalizedL1Data;
use alloy_primitives::B256;
use axum::async_trait;
use reqwest::Response;
use ssz_rs::Bitvector;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientFinalityUpdateResponse {
    pub data: LightClientFinalityUpdate,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientFinalityUpdate {
    pub finalized_header: LightClientHeader,
    pub sync_aggregate: SyncAggregate,
    #[serde(deserialize_with = "deserialize_u64_from_str")]
    pub signature_slot: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BeaconBlockHeader {
    #[serde(deserialize_with = "deserialize_u64_from_str")]
    pub slot: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientHeader {
    pub beacon: BeaconBlockHeader,
    pub execution: ExecutionPayloadHeader,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExecutionPayloadHeader {
    pub block_hash: B256,
    #[serde(deserialize_with = "deserialize_u64_from_str")]
    pub block_number: u64,
}

#[derive(Clone, serde::Deserialize)]
pub struct SyncAggregate {
    #[cfg(feature = "minimal")]
    pub sync_committee_bits: Bitvector<32>,
    #[cfg(not(feature = "minimal"))]
    pub sync_committee_bits: Bitvector<512>,
}

impl std::fmt::Debug for SyncAggregate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncAggregate")
            .field("participants", &self.sync_committee_bits.count_ones())
            .field("validators", &self.sync_committee_bits.len())
            .finish()
    }
}

impl SyncAggregate {
    pub fn is_insufficient_participation(&self) -> bool {
        let participants = self.sync_committee_bits.count_ones() as u64;
        let validators = self.sync_committee_bits.len() as u64;
        const TRUST_LEVEL_NUMERATOR: u64 = 2;
        const TRUST_LEVEL_DENOMINATOR: u64 = 3;
        participants * TRUST_LEVEL_DENOMINATOR < validators * TRUST_LEVEL_NUMERATOR
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

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientUpdateResponse {
    pub data: LightClientUpdate,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientUpdate {
    pub finalized_header: LightClientHeader,
    pub sync_aggregate: SyncAggregate,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GenesisResponse {
    pub data: GenesisData,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GenesisData {
    #[serde(deserialize_with = "deserialize_u64_from_str")]
    pub genesis_time: u64,
}

#[async_trait]
pub trait BeaconClient: Send + Sync + 'static {
    /// Returns parsed finality update and raw JSON value for storage.
    async fn get_light_client_finality_update(
        &self,
    ) -> anyhow::Result<(LightClientFinalityUpdateResponse, serde_json::Value)>;

    /// Returns parsed light client update and raw JSON value for storage.
    async fn get_light_client_update(
        &self,
        period: u64,
    ) -> anyhow::Result<(LightClientUpdateResponse, serde_json::Value)>;

    async fn get_genesis(&self) -> anyhow::Result<GenesisData>;

    /// Fetches finality update and light client update, returning FinalizedL1Data along with parsed responses.
    /// Calls get_light_client_finality_update and get_light_client_update internally to minimize lag between API calls.
    async fn get_finalized_l1_data(
        &self,
    ) -> anyhow::Result<(
        FinalizedL1Data,
        LightClientFinalityUpdateResponse,
        LightClientUpdateResponse,
    )> {
        let (finality_update, finality_value) = self.get_light_client_finality_update().await?;

        let finalized_slot = finality_update.data.finalized_header.beacon.slot;
        let finalized_period = compute_period_from_slot(finalized_slot);

        let (light_client_update, light_client_update_value) =
            self.get_light_client_update(finalized_period).await?;

        let finalized_l1_data = FinalizedL1Data {
            raw_finality_update: finality_value,
            raw_light_client_update: light_client_update_value,
            period: finalized_period,
        };

        Ok((finalized_l1_data, finality_update, light_client_update))
    }
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
    async fn get_light_client_finality_update(
        &self,
    ) -> anyhow::Result<(LightClientFinalityUpdateResponse, serde_json::Value)> {
        let response = self
            .client
            .get(format!(
                "{}/eth/v1/beacon/light_client/finality_update",
                self.beacon_addr
            ))
            .send()
            .await?;
        let response = self.check_response(response).await?;
        let text = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get finality update: {e:?}"))?;

        let raw_value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse finality update as JSON: {e:?}"))?;
        let parsed: LightClientFinalityUpdateResponse =
            serde_json::from_value(raw_value.clone())
                .map_err(|e| anyhow::anyhow!("Failed to parse finality update: {e:?}"))?;

        Ok((parsed, raw_value))
    }

    async fn get_light_client_update(
        &self,
        period: u64,
    ) -> anyhow::Result<(LightClientUpdateResponse, serde_json::Value)> {
        let response = self
            .client
            .get(format!(
                "{}/eth/v1/beacon/light_client/updates?start_period={}&count=1",
                self.beacon_addr, period
            ))
            .send()
            .await?;
        let response = self.check_response(response).await?;
        let text = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get light client update: {e:?}"))?;

        // The response is an array, extract the first element
        let updates: Vec<serde_json::Value> = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse light client updates: {e:?}"))?;

        if updates.is_empty() {
            return Err(anyhow::anyhow!(
                "No light client update found for period {period}"
            ));
        }

        let raw_value = updates.into_iter().next().unwrap();
        let parsed: LightClientUpdateResponse = serde_json::from_value(raw_value.clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse light client update: {e:?}"))?;

        Ok((parsed, raw_value))
    }

    async fn get_genesis(&self) -> anyhow::Result<GenesisData> {
        let response = self
            .client
            .get(format!("{}/eth/v1/beacon/genesis", self.beacon_addr))
            .send()
            .await?;
        let response = self.check_response(response).await?;
        let genesis: GenesisResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get genesis: {e:?}"))?;
        Ok(genesis.data)
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
    fn test_is_insufficient_participation_all_participants() {
        // 100% participation should NOT be insufficient
        #[cfg(feature = "minimal")]
        let agg = create_sync_aggregate_with_participation(32);
        #[cfg(not(feature = "minimal"))]
        let agg = create_sync_aggregate_with_participation(512);

        assert!(!agg.is_insufficient_participation());
    }

    #[test]
    fn test_is_insufficient_participation_two_thirds() {
        // Exactly 2/3 participation should NOT be insufficient (participants >= validators * 2/3)
        #[cfg(feature = "minimal")]
        let agg = create_sync_aggregate_with_participation(22); // 22/32 = 68.75% >= 66.67%
        #[cfg(not(feature = "minimal"))]
        let agg = create_sync_aggregate_with_participation(342); // 342/512 = 66.80% >= 66.67%

        assert!(!agg.is_insufficient_participation());
    }

    #[test]
    fn test_is_insufficient_participation_below_threshold() {
        // Less than 2/3 participation should be insufficient
        #[cfg(feature = "minimal")]
        let agg = create_sync_aggregate_with_participation(20); // 20/32 = 62.5% < 66.67%
        #[cfg(not(feature = "minimal"))]
        let agg = create_sync_aggregate_with_participation(340); // 340/512 = 66.41% < 66.67%

        assert!(agg.is_insufficient_participation());
    }

    #[test]
    fn test_is_insufficient_participation_no_participants() {
        // 0 participation should be insufficient
        let agg = create_sync_aggregate_with_participation(0);
        assert!(agg.is_insufficient_participation());
    }

    #[test]
    fn test_is_insufficient_participation_boundary() {
        // Test the exact boundary: participants * 3 >= validators * 2
        // For 512 validators: 512 * 2 / 3 = 341.33..., so need 342 participants
        // For 32 validators: 32 * 2 / 3 = 21.33..., so need 22 participants
        #[cfg(feature = "minimal")]
        {
            let agg_sufficient = create_sync_aggregate_with_participation(22);
            let agg_insufficient = create_sync_aggregate_with_participation(21);
            assert!(!agg_sufficient.is_insufficient_participation());
            assert!(agg_insufficient.is_insufficient_participation());
        }
        #[cfg(not(feature = "minimal"))]
        {
            let agg_sufficient = create_sync_aggregate_with_participation(342);
            let agg_insufficient = create_sync_aggregate_with_participation(341);
            assert!(!agg_sufficient.is_insufficient_participation());
            assert!(agg_insufficient.is_insufficient_participation());
        }
    }
}
