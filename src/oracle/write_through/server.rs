use std::fmt::Debug;
use std::sync::{Arc};
use kona_host::kv::KeyValueStore;
use tokio::task::JoinHandle;
use crate::oracle::write_through::fetcher::Fetcher;
use crate::oracle::write_through::HintSender;

pub fn start_hint_server<KV: KeyValueStore + ?Sized + Send + Sync + 'static>(
    mut hint_cache: Vec<String>,
    fetcher: Arc<Fetcher<KV>>,
) -> (
    async_channel::Sender<(String, HintSender)>,
    JoinHandle<Vec<String>>
){
    let (channel_sender, mut channel_receiver) = async_channel::unbounded::<(String, HintSender)>();
    let fetcher = fetcher.clone();
    let receiver =  channel_receiver.clone();
    let task = tokio::spawn(async move {
        loop {
            if let Ok((hint ,sender)) = receiver.recv().await {
                if hint_cache.contains(&hint) {
                    let _ = sender.send(false);
                    continue
                }
                let _ = fetcher.prefetch(&hint).await;
                hint_cache.push(hint.to_string());
                let _ = sender.send(true);
            }else {
                break
            }
        }
        return hint_cache;
    });
    (channel_sender, task)
}