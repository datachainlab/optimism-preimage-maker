use alloy_primitives::B256;
use axum::async_trait;
use reqwest::Response;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientFinalityUpdateResponse {
    pub data: LightClientFinalityUpdate,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightClientFinalityUpdate {
    pub finalized_header: LightClientHeader,
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
