//! Contains a concrete implementation of the [KeyValueStore] trait that stores data in memory.

use alloy_primitives::B256;
use anyhow::Result;
use std::collections::HashMap;
use kona_host::KeyValueStore;
use kona_preimage::PreimageKey;

pub trait Clearable: KeyValueStore {
    /// Clear the store, removing all entries.
    fn clear(&mut self);
}

/// A simple, synchronous key-value store that stores data in memory. This is useful for testing and
/// development purposes.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct MyMemoryKeyValueStore {
    /// The underlying store.
    pub store: HashMap<B256, Vec<u8>>,
}

impl Clearable for MyMemoryKeyValueStore {
    fn clear(&mut self) {
        self.store.clear()
    }
}

impl MyMemoryKeyValueStore {
    /// Create a new [MemoryKeyValueStore] with an empty store.
    pub fn new() -> Self {
        Self { store: HashMap::default() }
    }

}

impl KeyValueStore for MyMemoryKeyValueStore {
    fn get(&self, key: B256) -> Option<Vec<u8>> {
        self.store.get(&key).cloned()
    }

    fn set(&mut self, key: B256, value: Vec<u8>) -> Result<()> {
        self.store.insert(key, value);
        Ok(())
    }
}

use kona_preimage::PreimageKeyType;

/// A split implementation of the [KeyValueStore] trait that splits between two separate
/// [KeyValueStore]s.
#[derive(Clone, Debug)]
pub struct MySplitKeyValueStore<L, R>
where
    L: KeyValueStore,
    R: Clearable,
{
    local_store: L,
    remote_store: R,
}

impl<L, R> MySplitKeyValueStore<L, R>
where
    L: KeyValueStore,
    R: Clearable,
{
    /// Create a new [SplitKeyValueStore] with the given left and right [KeyValueStore]s.
    pub const fn new(local_store: L, remote_store: R) -> Self {
        Self { local_store, remote_store }
    }
}

impl<L, R> KeyValueStore for MySplitKeyValueStore<L, R>
where
    L: KeyValueStore,
    R: Clearable,
{
    fn get(&self, key: B256) -> Option<Vec<u8>> {
        match PreimageKeyType::try_from(key[0]).ok()? {
            PreimageKeyType::Local => self.local_store.get(key),
            _ => self.remote_store.get(key),
        }
    }

    fn set(&mut self, key: B256, value: Vec<u8>) -> Result<()> {
        self.remote_store.set(key, value)
    }
}

impl <L, R> Clearable for MySplitKeyValueStore<L, R>
where
    L: KeyValueStore,
    R: Clearable,
{
    fn clear(&mut self) {
        self.remote_store.clear()
    }
}