use std::fmt::Debug;
use std::sync::{Arc, };
use alloy_primitives::B256;
use kona_host::kv::KeyValueStore;
use kona_preimage::{HintWriterClient, PreimageFetcher, };
use tokio::sync::{RwLock};
use tokio::task::JoinHandle;
use crate::derivation::oracle::lockfree::fetcher::Fetcher;
use crate::derivation::oracle::lockfree::{HintSender, PreimageSender};

const SERVER_NUM: usize = 1;

pub fn start_preimage_server<KV: KeyValueStore + ?Sized + Send + Sync + 'static>(
    fetcher: Arc<RwLock<Fetcher<KV>>>,
) -> (
    async_channel::Sender<(B256, PreimageSender)>,
    Vec<JoinHandle<()>>
){
    let (channel_sender, mut channel_receiver) = async_channel::unbounded::<(B256, PreimageSender)>();
    let mut tasks = Vec::with_capacity(SERVER_NUM);
    for _ in 0..SERVER_NUM {
        let fetcher = fetcher.clone();
        let receiver =  channel_receiver.clone();
        let task = tokio::spawn(async move {
            loop {
                if let Ok((key, sender)) = receiver.recv().await {
                    let result = fetcher.read().await.get_preimage(key).await;
                    let _ = sender.send(result);
                } else {
                    break
                }
            }
        });
        tasks.push(task);
    }
    (channel_sender, tasks)
}

pub fn start_hint_server<KV: KeyValueStore + ?Sized + Send + Sync + 'static>(
    fetcher: Arc<RwLock<Fetcher<KV>>>,
) -> (
    async_channel::Sender<(String, HintSender)>,
    Vec<JoinHandle<()>>
){
    let (channel_sender, mut channel_receiver) = async_channel::unbounded::<(String, HintSender)>();
    let mut tasks = Vec::with_capacity(SERVER_NUM);
    for _ in 0..SERVER_NUM {
        let fetcher = fetcher.clone();
        let receiver =  channel_receiver.clone();
        let task = tokio::spawn(async move {
            loop {
                if let Ok((hint ,sender)) = receiver.recv().await {
                    fetcher.write().await.hint(&hint);
                    let _ = sender.send(true);
                }else {
                    break
                }
            }
        });
        tasks.push(task);
    }
    (channel_sender, tasks)
}