use optimism_derivation::types::Preimages;
use optimism_preimage_maker::derivation::host::single::config::Config;
use optimism_preimage_maker::derivation::host::single::handler::{
    Derivation, DerivationConfig, DerivationRequest,
};
use prost::Message;
use serial_test::serial;
use std::env;
use std::sync::Arc;
use tracing::info;

mod e2e;
use crate::e2e::derivation_in_light_client;
use e2e::get_l2_client;
use e2e::init;
use optimism_preimage_maker::client::l2_client::{Block, RpcRequest, RpcResult};
use optimism_preimage_maker::data::preimage_repository::PreimageMetadata;

async fn get_block_by_number(number: u64, l1_geth_addr: &str) -> anyhow::Result<Block> {
    let client = reqwest::Client::new();
    let body = RpcRequest {
        method: "eth_getBlockByNumber".into(),
        params: vec![format!("0x{number:X}").into(), false.into()],
        ..Default::default()
    };
    let response = client
        .post(l1_geth_addr)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "failed to get block by number: {response:?}"
        ));
    }
    let result: RpcResult<Block> = response.json().await?;
    Ok(result.result)
}

/*
ex)
export L2_ROLLUP_ADDR=http://localhost:9545
export L2_GETH_ADDR=http://localhost:8546
export L1_GETH_ADDR=http://localhost:8545
export L1_BEACON_ADDR=http://localhost:9596
export CLAIMED=104
export AGREED=103
*/
#[serial]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn test_derivation() {
    init();
    let claimed: u64 = env::var("CLAIMED").unwrap().parse().unwrap();
    let agreed: u64 = env::var("AGREED").unwrap().parse().unwrap();
    let l2_client = get_l2_client();
    let chain_id = l2_client.chain_id().await.unwrap();

    let op_geth_addr = env::var("L2_GETH_ADDR").unwrap();
    let op_node_addr = env::var("L2_ROLLUP_ADDR").unwrap();
    let l1_geth_addr = env::var("L1_GETH_ADDR").unwrap();
    let l1_beacon_addr = env::var("L1_BEACON_ADDR").unwrap();

    let claimed_output = l2_client.output_root_at(claimed).await.unwrap();
    let agreed_output = l2_client.output_root_at(agreed).await.unwrap();
    let l1_hash = get_block_by_number(
        claimed_output.block_ref.l1_origin.number + 50,
        &l1_geth_addr,
    )
    .await
    .unwrap()
    .hash;
    let request = DerivationRequest {
        l1_head_hash: l1_hash,
        agreed_l2_head_hash: agreed_output.block_ref.hash,
        agreed_l2_output_root: agreed_output.output_root,
        l2_output_root: claimed_output.output_root,
        l2_block_number: claimed,
    };

    let config = Arc::new(DerivationConfig {
        config: Config {
            l2_node_address: op_geth_addr.to_string(),
            l1_node_address: l1_geth_addr.to_string(),
            l1_beacon_address: l1_beacon_addr.to_string(),
            l2_rollup_address: op_node_addr.to_string(),
            ..Default::default()
        },
        rollup_config: None,
        l2_chain_id: chain_id,
        l1_chain_config: None,
    });

    info!("start derivation in preimage maker: {:?}", &request);
    let derivation = Derivation { config, request };
    let result = derivation.start().await;
    let preimage = match result {
        Ok(preimage) => {
            info!("derivation success");
            Preimages::decode(preimage.as_slice()).unwrap()
        }
        Err(e) => panic!("derivation failed: {e:?}"),
    };

    derivation_in_light_client(
        &l2_client,
        preimage,
        PreimageMetadata {
            agreed,
            claimed,
            l1_head: l1_hash,
        },
    )
    .await;
}
