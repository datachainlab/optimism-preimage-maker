use std::fs;
use optimism_derivation::derivation::Derivation;
use optimism_preimage_maker::derivation::client::l2;
#[tokio::test]
async fn test_makpreimages() {
    let op_node_addr = "http://localhost:7545".to_string();
    let op_geth_addr = "http://localhost:9545".to_string();
    let l2_client = l2::L2Client::new(op_node_addr.to_string(), op_geth_addr.to_string());

    let sync_status = l2_client.sync_status().await.unwrap();
    let mut derivations = vec![];
    for i in 0..10 {
        let n = 10 - i;
        let finalized_l2 = sync_status.finalized_l2.number - n;
        let claiming_l2_number = finalized_l2;
        let claiming_l2_hash = l2_client.get_block_by_number(claiming_l2_number).await.unwrap().hash;
        let claiming_output_root = l2_client.output_root_at(claiming_l2_number).await.unwrap();
        let agreed_l2_hash = l2_client.get_block_by_number(claiming_l2_number - 1).await.unwrap().hash;
        let agreed_output_root = l2_client.output_root_at(claiming_l2_number - 1).await.unwrap();
        derivations.push(Derivation::new(
            sync_status.finalized_l1.hash,
            agreed_l2_hash,
            agreed_output_root,
            claiming_l2_hash,
            claiming_output_root,
            claiming_l2_number,
        ));
    }

    let client = reqwest::Client::new();
    let builder = client.post("http://localhost:10080/derivation");
    let preimage_bytes = builder.json(&derivations).send().await.unwrap();
    let preimage_bytes = preimage_bytes.bytes().await.unwrap();
    fs::write("./derivations.json", serde_json::to_vec(&derivations).unwrap()).unwrap();
    fs::write("./preimage.bin", preimage_bytes).unwrap();
}