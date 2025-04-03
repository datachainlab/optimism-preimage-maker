use optimism_derivation::derivation::Derivation;
use optimism_derivation::oracle::MemoryOracleClient;
use optimism_derivation::types::Preimages;
use optimism_preimage_maker::{l2_client, Request};
use prost::Message;
use std::env;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::test(flavor = "multi_thread")]
async fn test_make_preimages() {
    let filter = filter::EnvFilter::from_default_env().add_directive("info".parse().unwrap());
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();

    let op_node_addr = format!("http://localhost:{}", env::var("L2_ROLLUP_PORT").unwrap());
    let op_geth_addr = format!("http://localhost:{}", env::var("L2_GETH_PORT").unwrap());
    tracing::info!(
        "Starting with op_node_addr: {} op_geth_addr: {}",
        op_node_addr,
        op_geth_addr
    );

    let l2_client = l2_client::L2Client::new(op_node_addr.to_string(), op_geth_addr.to_string());

    const BEHIND: u64 = 10;
    const L2_COUNT: u64 = 20;
    let chain_id = l2_client.chain_id().await.unwrap();
    let sync_status = l2_client.sync_status().await.unwrap();
    let finalized_l2 = sync_status.finalized_l2.number;
    let claiming_l2_number = finalized_l2 - BEHIND;
    let agreed_l2_number = claiming_l2_number - L2_COUNT;
    let claiming_output = l2_client.output_root_at(claiming_l2_number).await.unwrap();
    let agreed_l2_hash = l2_client
        .get_block_by_number(agreed_l2_number)
        .await
        .unwrap()
        .hash;
    let agreed_output = l2_client.output_root_at(agreed_l2_number).await.unwrap();
    tracing::info!(
        "claimed_output: l1_origin={:?} l1={:?}",
        claiming_output.block_ref.l1_origin.number,
        sync_status.finalized_l1.number
    );

    let request = Request {
        l1_head_hash: sync_status.finalized_l1.hash,
        agreed_l2_head_hash: agreed_l2_hash,
        agreed_l2_output_root: agreed_output.output_root,
        l2_output_root: claiming_output.output_root,
        l2_block_number: claiming_l2_number,
    };
    tracing::info!("request: {:?}", request);

    let client = reqwest::Client::new();
    let builder = client.post("http://localhost:10080/derivation");
    let preimage_bytes = builder.json(&request).send().await.unwrap();
    let preimage_bytes = preimage_bytes.bytes().await.unwrap();
    let rollup_config = l2_client.rollup_config().await.unwrap();

    tracing::info!("start derivation ");
    let preimages = Preimages::decode(preimage_bytes).unwrap();
    let oracle = MemoryOracleClient::try_from(preimages.preimages).unwrap();
    let derivation = Derivation::new(
        request.l1_head_hash,
        request.agreed_l2_output_root,
        request.l2_output_root,
        request.l2_block_number,
    );

    let result = derivation.verify(chain_id, &rollup_config, oracle);
    match result {
        Ok(h) => tracing::info!("Derivation verified successfully {:? }", h),
        Err(e) => tracing::error!("Derivation verification failed: {:?}", e),
    }
}
