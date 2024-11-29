use alloy_primitives::B256;
use anyhow::Result;
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RpcResult<T> {
    pub jsonrpc: String,
    pub id: i64,
    pub result: T
}

#[derive(Debug, Clone, serde::Serialize)]
struct RpcRequest {
    jsonrpc: String,
    method: String,
    params: Vec<()>,
    id: i64
}

impl Default for RpcRequest {
    fn default() -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: "".into(),
            params: vec![],
            id: 1
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct L1Header {
    pub hash: B256,
    pub number: u64,
    #[serde(rename = "parentHash")]
    pub parent_hash: B256,
    pub timestamp: u64
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct L1Origin {
    pub hash: B256,
    pub number: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct L2Header {
    pub hash: B256,
    pub number: u64,
    #[serde(rename = "parentHash")]
    pub parent_hash: B256,
    pub timestamp: u64,
    pub l1origin: L1Origin,
    #[serde(rename = "sequenceNumber")]
    pub sequence_number: u64
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SyncStatus {
    pub current_l1: L1Header,
    pub current_l1_finalized: L1Header,
    pub head_l1: L1Header,
    pub safe_l1: L1Header,
    pub finalized_l1: L1Header,
    pub unsafe_l2: L2Header,
    pub safe_l2: L2Header,
    pub finalized_l2: L2Header,
    pub pending_safe_l2: L2Header,
}


pub struct L2Client {
    op_node_addr: String
}
impl Default for L2Client {
    fn default() -> Self {
        Self {
            op_node_addr: "http://localhost:7545".into()
        }
    }
}


impl L2Client {
    pub async fn sync_status(&self) -> Result<SyncStatus> {

        let client = reqwest::Client::new();
        let body = RpcRequest {
            method: "optimism_syncStatus".into(),
            ..Default::default()
        };
        let response  = client
            .post(format!("{}", self.op_node_addr))
            .header("Content-Type", "application/json")
            .json(&body).send().await?;
        let result : RpcResult<SyncStatus> = response.json().await?;
        Ok(result.result)
    }
}

#[cfg(test)]
mod test {
    use crate::l2::sync_status::L2Client;

    #[tokio::test]
    pub async fn test_sync_status() {
        let client =  L2Client::default();
        let result = client.sync_status().await.unwrap();
        println!("{:?}", result);
    }
}