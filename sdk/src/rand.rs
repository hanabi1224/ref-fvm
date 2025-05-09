// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_shared::clock::ChainEpoch;
use fvm_shared::randomness::RANDOMNESS_LENGTH;

use crate::{SyscallResult, sys};

/// Gets 32 bytes of randomness from the ticket chain.
/// The supplied output buffer must have at least 32 bytes of capacity.
/// If this syscall succeeds, exactly 32 bytes will be written starting at the
/// supplied offset.
pub fn get_chain_randomness(round: ChainEpoch) -> SyscallResult<[u8; RANDOMNESS_LENGTH]> {
    unsafe { sys::rand::get_chain_randomness(round) }
}

/// Gets 32 bytes of randomness from the beacon system (currently Drand).
/// The supplied output buffer must have at least 32 bytes of capacity.
/// If this syscall succeeds, exactly 32 bytes will be written starting at the
/// supplied offset.
pub fn get_beacon_randomness(round: ChainEpoch) -> SyscallResult<[u8; RANDOMNESS_LENGTH]> {
    unsafe { sys::rand::get_beacon_randomness(round) }
}
