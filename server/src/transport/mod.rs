pub mod reqwest_transport;

use core::str::FromStr;
use std::marker::PhantomData;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[doc(hidden)]
pub struct HttpConnect<T> {
    url: Url,
    _pd: PhantomData<T>,
}

impl<T> HttpConnect<T> {
    pub const fn new(url: Url) -> Self {
        Self {
            url,
            _pd: PhantomData,
        }
    }
}

impl<T> FromStr for HttpConnect<T> {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.parse()?))
    }
}

#[derive(Clone, Debug)]
pub struct Http<T> {
    client: T,
    url: Url,
}

impl<T> Http<T> {
    pub const fn with_client(client: T, url: Url) -> Self {
        Self { client, url }
    }
}
