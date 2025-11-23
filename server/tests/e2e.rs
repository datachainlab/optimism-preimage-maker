use optimism_derivation::derivation::Derivation;
use optimism_derivation::oracle::MemoryOracleClient;
use optimism_derivation::types::Preimages;
use optimism_preimage_maker::client::l2_client::L2Client;
use optimism_preimage_maker::data::preimage_repository::PreimageMetadata;
use optimism_preimage_maker::web::ListMetadataFromRequest;
use prost::Message;
use serial_test::serial;
use std::env;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn init() {
    let filter = filter::EnvFilter::from_default_env().add_directive("e2e=info".parse().unwrap());
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .try_init();
}
fn get_l2_client() -> L2Client {
    let op_node_addr = env::var("L2_ROLLUP_ADDR").unwrap();
    let op_geth_addr = env::var("L2_GETH_ADDR").unwrap();
    tracing::info!(
        "Starting with op_node_addr: {} op_geth_addr: {}",
        op_node_addr,
        op_geth_addr
    );
    L2Client::new(op_node_addr, op_geth_addr)
}

#[serial]
#[tokio::test(flavor = "multi_thread")]
async fn test_derivation_success() {
    init();
    let l2_client = get_l2_client();

    let client = reqwest::Client::new();
    let latest_metadata = client
        .post("http://localhost:10080/get_latest_metadata")
        .send()
        .await
        .unwrap();
    assert_eq!(latest_metadata.status(), 200);
    let latest_metadata: Option<PreimageMetadata> = latest_metadata.json().await.unwrap();
    let latest_metadata = latest_metadata.unwrap();

    let metadata_list = client
        .post("http://localhost:10080/list_metadata_from")
        .json(&ListMetadataFromRequest { gt_claimed: 103 })
        .send()
        .await
        .unwrap();
    assert_eq!(metadata_list.status(), 200);

    let metadata_list = metadata_list.json::<Vec<PreimageMetadata>>().await.unwrap();
    assert_eq!(metadata_list.last().unwrap(), &latest_metadata);

    let chain_id = l2_client.chain_id().await.unwrap();
    for metadata in metadata_list {
        tracing::info!("metadata: {:?}", metadata);
        let preimage_bytes = client
            .post("http://localhost:10080/get_preimage")
            .json(&metadata)
            .send()
            .await
            .unwrap();
        assert_eq!(preimage_bytes.status(), 200);
        let preimage_bytes = preimage_bytes.bytes().await.unwrap();
        let preimages = Preimages::decode(preimage_bytes).unwrap();
        let claimed_l2_output_root = l2_client.output_root_at(metadata.claimed).await.unwrap();
        let agreed_l2_output_root = l2_client.output_root_at(metadata.agreed).await.unwrap();

        let oracle = MemoryOracleClient::try_from(preimages.preimages).unwrap();
        let derivation = Derivation::new(
            metadata.l1_head,
            agreed_l2_output_root.output_root,
            claimed_l2_output_root.output_root,
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
}
