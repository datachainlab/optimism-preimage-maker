use std::sync::Arc;
use std::time::Duration;
use log::error;
use optimism_derivation::derivation::Derivation;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use crate::derivation::ChannelInterface;
use crate::derivation::client::l2::L2Client;

pub fn start_polling_task(
    op_node_addr: &str,
    op_geth_addr: &str,
    sender: Sender<ChannelInterface>
) -> JoinHandle<()>{
    let l2_client = L2Client::new(op_node_addr.to_string(), op_geth_addr.to_string());
    tokio::spawn(async move {
        loop {
            let sync_status = l2_client.sync_status().await.unwrap();
            //  info!("sync status {:?}", sync_status);
            for n in 0..10 {
                // TODO last saved
                let i = 10 - n;

                let claiming_l2_number = sync_status.finalized_l2.number - i;

                let claiming_l2_hash = l2_client.get_block_by_number(claiming_l2_number).await.unwrap().hash;
                let claiming_output_root = l2_client.output_root_at(claiming_l2_number).await.unwrap();

                let agreed_l2_hash = l2_client.get_block_by_number(sync_status.finalized_l2.number - i - 1).await.unwrap().hash;
                let agreed_output_root = l2_client.output_root_at(sync_status.finalized_l2.number - i - 1).await.unwrap();
                /*
                let fetcher = Fetcher::new(global_kv_store.clone(), l1_provider.clone(), blob_provider.clone(), l2_provider.clone(), agreed_l2_hash);;
                let oracle = PreimageIO::new(Arc::new(fetcher));
                 */
                let derivation = Derivation::new(
                    sync_status.finalized_l1.hash,
                    agreed_l2_hash,
                    agreed_output_root,
                    claiming_l2_hash,
                    claiming_output_root,
                    claiming_l2_number
                );
                sender.send((derivation, None)).await.unwrap();
            }
            tokio::time::sleep(Duration::from_secs(20)).await;
        }
    })
}