use tokio::sync::oneshot;

pub mod fetcher;
pub mod client;
pub mod server;

type HintSender = oneshot::Sender<bool>;
