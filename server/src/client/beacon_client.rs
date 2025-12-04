use std::fmt::format;
use alloy_primitives::B256;
use reqwest::Response;

#[derive(Debug, Clone)]
pub struct BeaconClient {
    beacon_addr: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientFinalityUpdateResponse {
    pub data: LightClientFinalityUpdate
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientFinalityUpdate {
    pub finalized_header: LightClientHeader
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientHeader {
    pub execution: ExecutionPayloadHeader
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExecutionPayloadHeader {
    pub block_hash: B256,
    pub block_number: u64
}

impl BeaconClient {
    pub fn new(beacon_addr: String) -> Self {
        Self { beacon_addr }
    }

    pub async fn get_raw_light_client_finality_update(&self) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let response = client
            .get(&format!("{}/eth/v1/beacon/light_client/finality_update", self.beacon_addr))
            .send()
            .await?;
        let response = self.check_response(response).await?;
        response.text().await.map_err(|e| anyhow::anyhow!("Failed to get finality update: {:?}", e))
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