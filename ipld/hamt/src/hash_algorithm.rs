// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::hash::Hasher;

use sha2::{Digest, Sha256 as Sha256Hasher};

use crate::{Hash, HashedKey};

/// Algorithm used as the hasher for the Hamt.
pub trait HashAlgorithm {
    fn hash<X>(key: &X) -> HashedKey
    where
        X: Hash + ?Sized;
}

/// Type is needed because the Sha256 hasher does not implement `std::hash::Hasher`
#[derive(Default)]
struct Sha2HasherWrapper(Sha256Hasher);

impl Hasher for Sha2HasherWrapper {
    fn finish(&self) -> u64 {
        // u64 hash not used in hamt
        0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}

/// Sha256 hashing algorithm used for hashing keys in the Hamt.
#[derive(Debug)]
pub enum Sha256 {}

impl HashAlgorithm for Sha256 {
    fn hash<X>(key: &X) -> HashedKey
    where
        X: Hash + ?Sized,
    {
        let mut hasher = Sha2HasherWrapper::default();
        key.hash(&mut hasher);
        hasher.0.finalize().into()
    }
}

#[cfg(feature = "identity")]
#[derive(Default)]
struct IdentityHasher {
    bz: HashedKey,
}
#[cfg(feature = "identity")]
impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        // u64 hash not used in hamt
        0
    }

    fn write(&mut self, bytes: &[u8]) {
        for (i, byte) in bytes.iter().take(self.bz.len()).enumerate() {
            self.bz[i] = *byte;
        }
    }
}

/// Identity hashing algorithm used for hashing keys in the Hamt. This should only be used
/// for testing. The hash is just the first 32 bytes of the serialized key.
#[cfg(feature = "identity")]
#[derive(Debug)]
pub enum Identity {}

#[cfg(feature = "identity")]
impl HashAlgorithm for Identity {
    fn hash<X: ?Sized>(key: &X) -> HashedKey
    where
        X: Hash,
    {
        let mut ident_hasher = IdentityHasher::default();
        key.hash(&mut ident_hasher);
        ident_hasher.bz
    }
}
