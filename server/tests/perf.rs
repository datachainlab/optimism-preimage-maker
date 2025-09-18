use alloy_primitives::B256;
use optimism_preimage_maker::l2_client::L2Client;
use optimism_preimage_maker::Request;
use serial_test::serial;
use std::env;
use std::time::Instant;
use tokio::time;
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
    let op_node_addr = env::var("L2_ROLLUP").unwrap();
    let op_geth_addr = env::var("L2_GETH").unwrap();
    tracing::info!(
        "Starting with op_node_addr: {} op_geth_addr: {}",
        op_node_addr,
        op_geth_addr
    );
    L2Client::new(op_node_addr, op_geth_addr)
}

async fn get_latest_derivation(l2_client: &L2Client) -> (B256, u64) {
    let sync_status = l2_client.sync_status().await.unwrap();
    (
        sync_status.finalized_l1.hash,
        sync_status.finalized_l2.number,
    )
}

#[serial]
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Ignored because it's a performance test, not a unit test
async fn run_parallel() {
    let server_url = format!("{}/derivation", env::var("SERVER_URL").unwrap());
    let parallel: u64 = env::var("TASKS")
        .unwrap_or("1".to_string())
        .parse()
        .unwrap();
    init();
    let l2_client = get_l2_client();
    let mut last_claimed: Option<u64> = None;
    const MAX_BLOCK_NUMS_PER_CALL: u64 = 60;
    loop {
        time::sleep(time::Duration::from_secs(10)).await;
        let (l1_head, claiming_l2) = get_latest_derivation(&l2_client).await;
        if let Some(last_block) = last_claimed {
            if claiming_l2 == last_block {
                continue;
            }
        }
        last_claimed = Some(claiming_l2);

        let mut requests = vec![];
        for i in 0..parallel {
            let claiming_l2 = claiming_l2 - (i * MAX_BLOCK_NUMS_PER_CALL);
            let agreed_l2 = claiming_l2 - MAX_BLOCK_NUMS_PER_CALL;
            let agreed_output = l2_client.output_root_at(agreed_l2).await.unwrap();
            let claimed_output_root = l2_client.output_root_at(claiming_l2).await.unwrap();
            let request = Request {
                l1_head_hash: l1_head,
                agreed_l2_head_hash: agreed_output.block_ref.hash,
                agreed_l2_output_root: agreed_output.output_root,
                l2_output_root: claimed_output_root.output_root,
                l2_block_number: claiming_l2,
            };
            requests.push(request);
        }
        let mut tasks = vec![];
        for request in requests {
            let server_url = server_url.clone();
            let agreed_l2 = request.agreed_l2_head_hash;
            let task = tokio::spawn(async move {
                let request = request.clone();
                let client = reqwest::Client::new();
                let builder = client.post(server_url);
                let start = Instant::now();
                let preimage_bytes = builder.json(&request).send().await.unwrap();
                let elapsed = start.elapsed();
                let preimage_bytes = preimage_bytes.bytes().await.unwrap();
                tracing::info!(
                    "{}-{},{},{}",
                    request.l2_block_number,
                    agreed_l2,
                    elapsed.as_secs(),
                    preimage_bytes.len()
                );
            });
            tasks.push(task);
        }
        for task in tasks {
            if let Err(e) = task.await {
                tracing::error!("Task failed: {:?}", e);
            }
        }
    }
}
