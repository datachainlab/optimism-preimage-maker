use std::env;
use optimism_derivation::derivation::Derivation;
use optimism_derivation::oracle::MemoryOracleClient;
use optimism_derivation::types::Preimages;
use prost::Message;
use serial_test::serial;
use tokio::time;
use tokio::time::Instant;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use optimism_preimage_maker::l2_client::L2Client;
use optimism_preimage_maker::Request;

fn init() {
    let filter = filter::EnvFilter::from_default_env().add_directive("info".parse().unwrap());
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .try_init();
}
fn get_l2_client() -> L2Client {
    let op_node_addr = format!("{}", env::var("L2_ROLLUP_").unwrap());
    let op_geth_addr = format!("{}", env::var("L2_GETH").unwrap());
    tracing::info!(
        "Starting with op_node_addr: {} op_geth_addr: {}",
        op_node_addr,
        op_geth_addr
    );
    L2Client::new(op_node_addr, op_geth_addr)
}

async fn get_latest_derivation(l2_client: &L2Client) -> (Request, u64) {
    const BEHIND: u64 = 180;
    const L2_COUNT: u64 = 30;
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
    (Request {
        l1_head_hash: sync_status.finalized_l1.hash,
        agreed_l2_head_hash: agreed_l2_hash,
        agreed_l2_output_root: agreed_output.output_root,
        l2_output_root: claiming_output.output_root,
        l2_block_number: claiming_l2_number,
    }, agreed_l2_number)
}

#[serial]
#[tokio::test(flavor = "multi_thread")]
async fn run_performance() {
    init();
    let l2_client = get_l2_client();
    let mut last: Option<u64> = None;
    loop {
        time::sleep(time::Duration::from_secs(3)).await;
        let (request , agreed) = get_latest_derivation(&l2_client).await;
        if let Some(last_block) = last {
            if request.l2_block_number == last_block {
                continue;
            }
        }
        last = Some(request.l2_block_number);
        let client = reqwest::Client::new();
        let builder = client.post("http://localhost:10080/derivation");
        let start = Instant::now();
        let preimage_bytes = builder.json(&request).send().await.unwrap();
        let elapsed = start.elapsed();
        let preimage_bytes = preimage_bytes.bytes().await.unwrap();
        tracing::info!("{}-{},{},{}", request.l2_block_number, agreed, elapsed.as_secs(), preimage_bytes.len());
    }
}