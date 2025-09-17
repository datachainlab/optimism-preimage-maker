pub mod reqwest_transport;

use url::Url;

#[derive(Clone, Debug)]
pub struct Http<T> {
    client: T,
    url: Url,
}
