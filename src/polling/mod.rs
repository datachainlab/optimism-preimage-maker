use std::cmp::min;
use crate::checkpoint::LastBlock;
use crate::derivation::client::l2::L2Client;
use crate::derivation::ChannelInterface;
use optimism_derivation::derivation::Derivation;
use std::time::Duration;
use log::info;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

pub fn start_polling_task(
    last_block: LastBlock,
    op_node_addr: &str,
    op_geth_addr: &str,
    sender: Sender<ChannelInterface>,
) -> JoinHandle<()> {
    let l2_client = L2Client::new(op_node_addr.to_string(), op_geth_addr.to_string());
    tokio::spawn(async move {
        let mut prev = last_block.l2_block_number + 1;
        loop {
            let sync_status = l2_client.sync_status().await.unwrap();
            let finalized_l2 = sync_status.finalized_l2.number;
            info!("sync status: l1={:?}, l2={}", sync_status.finalized_l1.hash, finalized_l2);
            for n in (prev + 1)..finalized_l2 {
                let claiming_l2_number = n;
                let claiming_l2_hash = l2_client
                    .get_block_by_number(claiming_l2_number)
                    .await
                    .unwrap()
                    .hash;
                let claiming_output_root =
                    l2_client.output_root_at(claiming_l2_number).await.unwrap();

                let agreed_l2_hash = l2_client.get_block_by_number(n - 1).await.unwrap().hash;
                let agreed_output_root = l2_client.output_root_at(n - 1).await.unwrap();
                let derivation = Derivation::new(
                    sync_status.finalized_l1.hash,
                    agreed_l2_hash,
                    agreed_output_root,
                    claiming_l2_hash,
                    claiming_output_root,
                    claiming_l2_number,
                );
                sender.send((vec![derivation], None)).await.unwrap();
                prev = claiming_l2_number
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
}
