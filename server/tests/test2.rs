use optimism_preimage_maker::{l2_client, Request};
use std::fs;
use std::time::Duration;

#[tokio::test]
async fn test_make_preimages() {
    let op_node_addr = "http://localhost:7545".to_string();
    let op_geth_addr = "http://localhost:9545".to_string();
    let l2_client = l2_client::L2Client::new(op_node_addr.to_string(), op_geth_addr.to_string());

    const L2_COUNT: u64 = 100;
    let mut sync_status = l2_client.sync_status().await.unwrap();

    let mut finalized_l2 = sync_status.finalized_l2.number;
    let agreed_l2_number = finalized_l2 - L2_COUNT;
    let agreed_l2_hash = l2_client
        .get_block_by_number(agreed_l2_number)
        .await
        .unwrap()
        .hash;
    let agreed_output = l2_client.output_root_at(agreed_l2_number).await.unwrap();

    loop {
        let claiming_l2_number = finalized_l2;
        let claiming_output = l2_client.output_root_at(claiming_l2_number).await.unwrap();
        println!(
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
        println!("request: {:?}", request);

        let client = reqwest::Client::new();
        let builder = client.post("http://localhost:10080/derivation");
        let preimage_bytes = builder.json(&request).send().await.unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
        sync_status = l2_client.sync_status().await.unwrap();
        finalized_l2 = sync_status.finalized_l2.number;
    }
}
