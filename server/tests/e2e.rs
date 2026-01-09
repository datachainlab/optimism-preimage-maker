//! These tests are required to run the preimage server.

use optimism_derivation::derivation::Derivation;
use optimism_derivation::oracle::MemoryOracleClient;
use optimism_derivation::types::Preimages;
use optimism_preimage_maker::client::beacon_client::LightClientFinalityUpdateResponse;
use optimism_preimage_maker::client::l2_client::{HttpL2Client, L2Client};
use optimism_preimage_maker::data::preimage_repository::PreimageMetadata;
use optimism_preimage_maker::web::{
    GetFinalizedL1Request, GetPreimageRequest, ListMetadataRequest,
};
use prost::Message;
use std::env;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init() {
    let filter = filter::EnvFilter::from_default_env().add_directive("info".parse().unwrap());
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .try_init();
}

pub fn get_l2_client() -> HttpL2Client {
    let op_node_addr = env::var("L2_ROLLUP_ADDR").unwrap();
    let op_geth_addr = env::var("L2_GETH_ADDR").unwrap();
    tracing::info!(
        "Starting with op_node_addr: {} op_geth_addr: {}",
        op_node_addr,
        op_geth_addr
    );
    HttpL2Client::new(
        op_node_addr,
        op_geth_addr,
        std::time::Duration::from_secs(30),
    )
}

pub async fn derivation_in_light_client(
    l2_client: &HttpL2Client,
    preimages: Preimages,
    metadata: PreimageMetadata,
) {
    let agreed_l2_output_root = l2_client
        .output_root_at(metadata.agreed)
        .await
        .unwrap()
        .output_root;
    let claimed_l2_output_root = l2_client
        .output_root_at(metadata.claimed)
        .await
        .unwrap()
        .output_root;
    let chain_id = l2_client.chain_id().await.unwrap();

    let oracle = MemoryOracleClient::try_from(preimages.preimages).unwrap();
    let derivation = Derivation::new(
        metadata.l1_head,
        agreed_l2_output_root,
        claimed_l2_output_root,
        metadata.claimed,
    );
    tracing::info!("start derivation {:?}", derivation);

    let result = derivation.verify(chain_id, oracle);
    match result {
        Ok(h) => tracing::info!("Derivation verified successfully {:? }", h),
        Err(e) => {
            tracing::error!("Derivation verification failed: {:?}", e);
            panic!("Derivation verification failed");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
pub async fn test_derivation_success() {
    init();
    let l2_client = get_l2_client();

    let client = reqwest::Client::new();
    let root_path = "http://localhost:10080";
    let latest_metadata: PreimageMetadata = client
        .post(format!("{root_path}/get_latest_metadata"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let metadata_list: Vec<PreimageMetadata> = client
        .post(format!("{root_path}/list_metadata"))
        .json(&ListMetadataRequest {
            lt_claimed: latest_metadata.claimed + 1,
            gt_claimed: 100,
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!metadata_list.is_empty(), "No metadata found");

    for metadata in metadata_list {
        assert!(metadata.claimed > 100);
        assert!(metadata.claimed < latest_metadata.claimed + 1);

        // Assert finalized l1
        let finalized_l1: LightClientFinalityUpdateResponse = client
            .post(format!("{root_path}/get_finalized_l1"))
            .json(&GetFinalizedL1Request {
                l1_head_hash: metadata.l1_head,
            })
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            finalized_l1.data.finalized_header.execution.block_hash,
            metadata.l1_head
        );

        // Get preimage
        let preimage_bytes = client
            .post(format!("{root_path}/get_preimage"))
            .json(&GetPreimageRequest {
                agreed: metadata.agreed,
                claimed: metadata.claimed,
                l1_head: metadata.l1_head,
            })
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();

        let preimages = Preimages::decode(preimage_bytes).unwrap();
        derivation_in_light_client(&l2_client, preimages, metadata).await;
    }
}
