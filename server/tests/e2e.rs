use optimism_derivation::derivation::Derivation;
use optimism_derivation::oracle::MemoryOracleClient;
use optimism_derivation::types::Preimages;
use optimism_preimage_maker::client::l2_client::L2Client;
use optimism_preimage_maker::Request;
use prost::Message;
use serial_test::serial;
use std::env;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn init() {
    let filter = filter::EnvFilter::from_default_env().add_directive("info".parse().unwrap());
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .try_init();
}
fn get_l2_client() -> L2Client {
    let op_node_addr = format!("http://localhost:{}", env::var("L2_ROLLUP_PORT").unwrap());
    let op_geth_addr = format!("http://localhost:{}", env::var("L2_GETH_PORT").unwrap());
    tracing::info!(
        "Starting with op_node_addr: {} op_geth_addr: {}",
        op_node_addr,
        op_geth_addr
    );
    L2Client::new(op_node_addr, op_geth_addr)
}

async fn get_latest_derivation(l2_client: &L2Client) -> Request {
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
        claiming_output.block_ref.l1_origin.number,
        sync_status.finalized_l1.number
    );

    Request {
        l1_head_hash: sync_status.finalized_l1.hash,
        agreed_l2_head_hash: agreed_l2_hash,
        agreed_l2_output_root: agreed_output.output_root,
        l2_output_root: claiming_output.output_root,
        l2_block_number: claiming_l2_number,
    }
}

#[serial]
#[tokio::test(flavor = "multi_thread")]
async fn test_make_preimages_success() {
    init();
    let l2_client = get_l2_client();

    let request = get_latest_derivation(&l2_client).await;
    tracing::info!("request: {:?}", request);

    let client = reqwest::Client::new();
    let builder = client.post("http://localhost:10080/derivation");
    let preimage_bytes = builder.json(&request).send().await.unwrap();
    assert_eq!(preimage_bytes.status(), 200);

    let preimage_bytes = preimage_bytes.bytes().await.unwrap();

    tracing::info!("start derivation ");
    let preimages = Preimages::decode(preimage_bytes).unwrap();
    let oracle = MemoryOracleClient::try_from(preimages.preimages).unwrap();
    let derivation = Derivation::new(
        request.l1_head_hash,
        request.agreed_l2_output_root,
        request.l2_output_root,
        request.l2_block_number,
    );

    let chain_id = l2_client.chain_id().await.unwrap();
    let result = derivation.verify(chain_id, oracle);
    match result {
        Ok(h) => tracing::info!("Derivation verified successfully {:? }", h),
        Err(e) => {
            tracing::error!("Derivation verification failed: {:?}", e);
            panic!("Derivation verification failed");
        }
    }
}

#[serial]
#[tokio::test(flavor = "multi_thread")]
async fn test_make_preimages_error() {
    init();
    let l2_client = get_l2_client();

    let mut request = get_latest_derivation(&l2_client).await;

    // invalid l2_output_root
    request.l2_output_root = request.agreed_l2_output_root;
    tracing::info!("request: {:?}", request);

    let client = reqwest::Client::new();
    let builder = client.post("http://localhost:10080/derivation");
    let result = builder.json(&request).send().await.unwrap();
    assert_eq!(
        result.status(),
        400,
        "Derivation should fail {}",
        result.status()
    );
}
