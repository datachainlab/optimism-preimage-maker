use alloy_primitives::B256;
use axum::async_trait;
use serde::Deserialize;

#[async_trait]
pub trait L1Client: Send + Sync + 'static {
    /// Returns the block number for a given block hash.
    async fn get_block_number_by_hash(&self, hash: B256) -> anyhow::Result<u64>;
}

#[derive(Debug, Clone)]
pub struct HttpL1Client {
    l1_node_address: String,
    client: reqwest::Client,
}

impl HttpL1Client {
    pub fn new(l1_node_address: String, timeout: std::time::Duration) -> Self {
        Self {
            l1_node_address,
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .expect("failed to build reqwest client"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct BlockResponse {
    #[serde(deserialize_with = "deserialize_u64_from_hex")]
    number: u64,
}

fn deserialize_u64_from_hex<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    let s = s.strip_prefix("0x").unwrap_or(&s);
    u64::from_str_radix(s, 16).map_err(serde::de::Error::custom)
}

#[async_trait]
impl L1Client for HttpL1Client {
    async fn get_block_number_by_hash(&self, hash: B256) -> anyhow::Result<u64> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByHash",
            "params": [format!("{:?}", hash), false],
            "id": 1
        });

        let response = self
            .client
            .post(&self.l1_node_address)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Request failed with status: {} body={:?}",
                response.status(),
                response.text().await
            ));
        }

        let json_response: JsonRpcResponse<BlockResponse> = response.json().await?;

        if let Some(error) = json_response.error {
            return Err(anyhow::anyhow!(
                "JSON-RPC error: code={} message={}",
                error.code,
                error.message
            ));
        }

        let block = json_response
            .result
            .ok_or_else(|| anyhow::anyhow!("Block not found for hash {:?}", hash))?;

        Ok(block.number)
    }
}
