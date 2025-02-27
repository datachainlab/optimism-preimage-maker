//! Contains a concrete implementation of the [KeyValueStore] trait that stores data in memory.

use alloy_primitives::B256;
use anyhow::Result;
use kona_host::KeyValueStore;
use kona_preimage::PreimageKey;
use optimism_derivation::types::{Preimage, Preimages};
use std::sync::{Arc, Mutex};

type Inner = Box<dyn KeyValueStore + Send + Sync>;

pub struct TracingKeyValueStore {
    pub inner: Inner,
    pub used: Arc<Mutex<hashbrown::HashMap<PreimageKey, Vec<u8>>>>,
}

impl TracingKeyValueStore {
    pub fn new(inner: Inner) -> Self {
        Self {
            inner,
            used: Default::default(),
        }
    }
}

impl KeyValueStore for TracingKeyValueStore {
    fn get(&self, key: B256) -> Option<Vec<u8>> {
        let v = self.inner.get(key);
        if let Some(value) = &v {
            let mut lock = self.used.lock().unwrap();
            let k = PreimageKey::try_from(key.0).unwrap();
            if !lock.contains_key(&k) {
                lock.insert(k, value.clone());
            }
        }
        v
    }

    fn set(&mut self, key: B256, value: Vec<u8>) -> Result<()> {
        self.inner.set(key, value.clone())?;
        let mut lock = self.used.lock().unwrap();
        lock.insert(PreimageKey::try_from(key.0)?, value);
        Ok(())
    }
}

pub fn encode_to_bytes(used: hashbrown::HashMap<PreimageKey, Vec<u8>>) -> Vec<u8> {
    let mut temp: Vec<Preimage> = Vec::with_capacity(used.len());
    for (k, v) in used.iter() {
        temp.push(Preimage::new(*k, v.clone()));
    }
    let data = Preimages { preimages: temp };
    data.into_vec().unwrap()
}
