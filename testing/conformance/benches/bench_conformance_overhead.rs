// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
extern crate criterion;
use std::env::var;
use std::path::Path;
use std::time::Duration;

use criterion::*;
use fvm::engine::MultiEngine;
use fvm::machine::BURNT_FUNDS_ACTOR_ID;
use fvm_conformance_tests::driver::*;
use fvm_conformance_tests::vector::{ApplyMessage, MessageVector};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use fvm_shared::message::Message;
use num_traits::Zero;
use walkdir::WalkDir;

mod bench_drivers;
use crate::bench_drivers::{CheckStrength, bench_vector_file};

/// benches only machine setup, no messages get sent. This is basically overhead of the benchmarks themselves.
fn bench_init_only(
    group: &mut BenchmarkGroup<measurement::WallTime>,
    path_to_setup: &Path,
    engines: &MultiEngine,
) -> anyhow::Result<()> {
    // compute measurement overhead by benching running a single empty vector of zero messages
    let mut message_vector = MessageVector::from_file(path_to_setup)?;
    if !message_vector.is_supported() {
        return Err(anyhow::anyhow!(
            "chosen vector was filtered out by selector"
        ));
    }
    message_vector.preconditions.variants.truncate(1);
    message_vector.apply_messages = Vec::new();
    bench_vector_file(
        group,
        &message_vector,
        CheckStrength::OnlyCheckSuccess,
        "bench_init_only",
        engines,
    )
}

/// benchmarks calling 500 simple state accesses. This benchmark computes the overhead of the message plus state access itself, doing a minimal amount of computation within the FVM.
fn bench_500_simple_state_access(
    group: &mut BenchmarkGroup<measurement::WallTime>,
    path_to_setup: &Path,
    engines: &MultiEngine,
) -> anyhow::Result<()> {
    let five_hundred_state_accesses = (0..500)
        .map(|i| ApplyMessage {
            bytes: fvm_ipld_encoding::to_vec(&Message {
                version: 0,
                from: Address::new_id(BURNT_FUNDS_ACTOR_ID),
                to: Address::new_id(BURNT_FUNDS_ACTOR_ID),
                sequence: i,
                value: TokenAmount::zero(),
                method_num: 2,
                params: RawBytes::default(),
                gas_limit: 5000000000,
                gas_fee_cap: TokenAmount::zero(),
                gas_premium: TokenAmount::zero(),
            })
            .unwrap(),
            epoch_offset: None,
        })
        .collect();

    let mut message_vector = MessageVector::from_file(path_to_setup)?;
    if !message_vector.is_supported() {
        return Err(anyhow::anyhow!(
            "chosen vector was filtered out by selector"
        ));
    }
    message_vector.preconditions.variants.truncate(1);
    message_vector.apply_messages = five_hundred_state_accesses;
    bench_vector_file(
        group,
        &message_vector,
        CheckStrength::OnlyCheckSuccess,
        "bench_500_simple_state_access",
        engines,
    )
}
/// runs overhead benchmarks, using the contents of the environment variable VECTOR as the starting FVM state
fn bench_conformance_overhead(c: &mut Criterion) {
    env_logger::init();

    let path_to_setup = match var("VECTOR") {
        Ok(v) => Path::new(v.as_str()).to_path_buf(),
        Err(_) => WalkDir::new("test-vectors/corpus")
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(is_runnable)
            .map(|e| e.path().to_path_buf())
            .next()
            .unwrap(),
    };

    let mut group = c.benchmark_group("measurement-overhead-baselines");
    group.measurement_time(Duration::new(15, 0));
    // start by getting some baselines!

    let engines = MultiEngine::default();
    bench_init_only(&mut group, &path_to_setup, &engines).unwrap();
    bench_500_simple_state_access(&mut group, &path_to_setup, &engines).unwrap();
    group.finish();
}

criterion_group!(benches_overhead, bench_conformance_overhead);
criterion_main!(benches_overhead);
