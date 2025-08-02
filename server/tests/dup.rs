use optimism_derivation::derivation::Derivation;
use optimism_derivation::oracle::MemoryOracleClient;
use optimism_derivation::types::Preimages;
use optimism_preimage_maker::l2_client::L2Client;
use optimism_preimage_maker::Request;
use prost::Message;
use serial_test::serial;
use std::env;
use tokio::time;
use tokio::time::Instant;
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
    let op_node_addr = format!("{}", env::var("L2_ROLLUP").unwrap());
    let op_geth_addr = format!("{}", env::var("L2_GETH").unwrap());
    tracing::info!(
        "Starting with op_node_addr: {} op_geth_addr: {}",
        op_node_addr,
        op_geth_addr
    );
    L2Client::new(op_node_addr, op_geth_addr)
}

async fn get_latest_derivations(l2_client: &L2Client) -> Vec<(Request, u64)> {
    const BEHIND: u64 = 20;
    let sync_status = l2_client.sync_status().await.unwrap();
    let finalized_l2 = sync_status.finalized_l2.number;
    let claiming_l2_number = finalized_l2 - BEHIND;

    let targets = vec![10, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
    let mut requests = vec![];
    for target in targets {
        let agreed_l2_number = claiming_l2_number - target;
        let claiming_output = l2_client.output_root_at(claiming_l2_number).await.unwrap();
        let agreed_l2_hash = l2_client
            .get_block_by_number(agreed_l2_number)
            .await
            .unwrap()
            .hash;
        let agreed_output = l2_client.output_root_at(agreed_l2_number).await.unwrap();
        requests.push((
            Request {
                l1_head_hash: sync_status.finalized_l1.hash,
                agreed_l2_head_hash: agreed_l2_hash,
                agreed_l2_output_root: agreed_output.output_root,
                l2_output_root: claiming_output.output_root,
                l2_block_number: claiming_l2_number,
            },
            agreed_l2_number,
        ));
    }
    return requests;
}

#[serial]
#[tokio::test(flavor = "multi_thread")]
async fn run_dup() {
    init();
    let l2_client = get_l2_client();
    let requests = get_latest_derivations(&l2_client).await;
    let mut preimages = vec![];
    for (request, agreed) in requests {
        let client = reqwest::Client::new();
        let builder = client.post("http://localhost:10080/derivation");
        let start = Instant::now();
        let preimage_bytes = builder.json(&request).send().await.unwrap();
        let elapsed = start.elapsed();
        let preimage_bytes = preimage_bytes.bytes().await.unwrap();
        let preimage = Preimages::decode(preimage_bytes.clone()).unwrap();
        tracing::info!(
            "{}-{},{},{},{}",
            request.l2_block_number,
            agreed,
            elapsed.as_secs(),
            preimage.preimages.len(),
            preimage_bytes.len()
        );
        preimages.push(preimage.preimages);
    }
    tracing::info!("check preimage");
    let (first, rest) = preimages.split_first().unwrap();
    let mut dup_count = vec![];
    for v in rest {
        dup_count.push(0);
    }

    for (j, preimage) in first.iter().enumerate() {
        if j % 1000 == 0 {
            tracing::info!("checking preimage {}", j);
        }
        for (i, other) in rest.iter().enumerate() {
            if other.iter().find(|x| x.key == preimage.key).is_some() {
                dup_count[i] += 1
            }
        }
    }
    for (i, images) in rest.iter().enumerate() {
        tracing::info!(
            "dup count {}: total_entry={}, exists_10={}",
            i,
            images.len(),
            dup_count[i]
        );
    }
}
