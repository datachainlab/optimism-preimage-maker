//! Contains a concrete implementation of the [KeyValueStore] trait that stores data in memory.

use alloy_primitives::B256;
use anyhow::Result;
use hashbrown::HashMap;
use kona_host::KeyValueStore;
use kona_preimage::PreimageKey;
use optimism_derivation::types::{Preimage, Preimages};
use crate::host::single::store::Clearable;

type Inner = Box<dyn Clearable + Send + Sync>;

pub struct TracingKeyValueStore {
    pub inner: Inner,
    pub used: hashbrown::HashMap<PreimageKey, Vec<u8>>,
}

impl TracingKeyValueStore {
    pub fn new(inner: Inner) -> Self {
        Self {
            inner,
            used: Default::default(),
        }
    }
    pub fn clear(&mut self) -> HashMap<PreimageKey, Vec<u8>> {
        self.inner.clear();
        std::mem::take(&mut self.used)
    }
}

impl KeyValueStore for TracingKeyValueStore {
    fn get(&self, key: B256) -> Option<Vec<u8>> {
        self.inner.get(key)
    }

    fn set(&mut self, key: B256, value: Vec<u8>) -> Result<()> {
        self.inner.set(key, value.clone())?;
        self.used.insert(PreimageKey::try_from(key.0)?, value);
        Ok(())
    }

}

pub fn encode_to_bytes(used: hashbrown::HashMap<PreimageKey, Vec<u8>>) -> Preimages {
    let mut temp: Vec<Preimage> = Vec::with_capacity(used.len());
    for (k, v) in used.into_iter() {
        temp.push(Preimage::new(k, v));
    }

    Preimages { preimages: temp }
}
