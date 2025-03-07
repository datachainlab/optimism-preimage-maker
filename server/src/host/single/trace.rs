//! Contains a concrete implementation of the [KeyValueStore] trait that stores data in memory.

use alloy_primitives::B256;
use anyhow::Result;
use kona_host::KeyValueStore;
use kona_preimage::PreimageKey;
use optimism_derivation::types::{Preimage, Preimages};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU64;
use tracing::{error, info};

type Inner = Box<dyn KeyValueStore + Send + Sync>;

pub struct TracingKeyValueStore {
    pub inner: Inner,
    pub used: hashbrown::HashMap<PreimageKey, Vec<u8>>,
    pub should_check: bool,
    pub get_count: AtomicU64,
    pub set_count: AtomicU64,
    pub used_raw: Vec<PreimageKey>
}

impl TracingKeyValueStore {
    pub fn new(inner: Inner) -> Self {
        Self {
            inner,
            used: Default::default(),
            should_check: false,
            get_count: AtomicU64::new(0),
            set_count: AtomicU64::new(0),
            used_raw: Vec::new()
        }
    }
}

impl KeyValueStore for TracingKeyValueStore {
    fn get(&self, key: B256) -> Option<Vec<u8>> {
        let v = self.inner.get(key);
        self.get_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.should_check {
            if v.is_none() {
                error!("not found in check mode: {:?}", key);
            }
        }
        v
    }

    fn set(&mut self, key: B256, value: Vec<u8>) -> Result<()> {
        self.set_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.should_check {
            return Ok(())
        }
        self.inner.set(key, value.clone())?;
        self.used.insert( PreimageKey::try_from(key.0)?, value);
        self.used_raw.push(PreimageKey::try_from(key.0)?);
        Ok(())
    }
}

pub fn encode_to_bytes(used: hashbrown::HashMap<PreimageKey, Vec<u8>>) -> Preimages {
    let mut temp: Vec<Preimage> = Vec::with_capacity(used.len());
    for (k, v) in used.iter() {
        temp.push(Preimage::new(*k, v.clone()));
    }
    let data = Preimages { preimages: temp };
    data
}
