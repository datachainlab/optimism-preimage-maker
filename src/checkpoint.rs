use anyhow::Result;
use std::fs;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct LastBlock {
    pub l2_block_number: u64,
}

/// Start task to save derivated block number
pub fn start_checkpoint_server(
    datadir: &str,
) -> Result<(JoinHandle<()>, Sender<LastBlock>, LastBlock)> {
    let path = format!("{}/last_block.txt", datadir);
    let last_block = match fs::read(path.clone()) {
        Err(err) => {
            tracing::warn!("failed to read last block: {:?}", err);
            LastBlock { l2_block_number: 0 }
        }
        Ok(data) => serde_json::from_slice(&data)?,
    };

    let (sender, mut receiver) = mpsc::channel::<LastBlock>(1);

    let task = tokio::spawn(async move {
        while let Some(data) = receiver.recv().await {
            match serde_json::to_vec(&data) {
                Err(e) => {
                    tracing::error!("failed to serialize last block: {:?}", e);
                }
                Ok(data) => {
                    if let Err(err) = fs::write(&path, data) {
                        tracing::error!("failed to save last block: {:?}", err);
                    }
                }
            }
        }
    });
    tracing::info!("Last saved block: {:?}", last_block);
    Ok((task, sender, last_block))
}
