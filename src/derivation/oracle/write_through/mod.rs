use tokio::sync::oneshot;

pub mod client;
pub mod fetcher;
pub mod server;

type HintSender = oneshot::Sender<bool>;
