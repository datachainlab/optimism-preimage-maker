use optimism_preimage_maker::{l2_client, Request};
use std::fs;

#[tokio::test]
async fn test_make_preimages() {
    let op_node_addr = "http://localhost:62265".to_string();
    let op_geth_addr = "http://localhost:62255".to_string();
    let l2_client = l2_client::L2Client::new(op_node_addr.to_string(), op_geth_addr.to_string());

    const BEHIND: u64 = 10;
    const L2_COUNT: u64 = 20;
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
        claiming_output.block_ref.l1_origin.number, sync_status.finalized_l1.number
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

    fs::create_dir("../testdata");
    fs::write(
        "../testdata/derivation.json",
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    fs::write("../testdata/preimage.bin", preimage_bytes).unwrap();
    fs::write(
        "../testdata/rollup_config.json",
        serde_json::to_vec(&rollup_config).unwrap(),
    )
    .unwrap();
}
