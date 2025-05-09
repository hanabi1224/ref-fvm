// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

extern crate serde;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_hamt::Hamt;

const ITEM_COUNT: u8 = 40;

// Struct to simulate a reasonable amount of data per value into the amt
#[derive(Clone, Serialize_tuple, Deserialize_tuple, PartialEq)]
struct BenchData {
    v1: Vec<u8>,
    v2: Vec<u8>,
    v3: Vec<u8>,
    v: u64,
    a: [u8; 32],
    a2: [u8; 32],
}

impl BenchData {
    fn new(val: u8) -> Self {
        Self {
            v1: vec![val; 8],
            v2: vec![val; 20],
            v3: vec![val; 10],
            v: 8,
            a: [val; 32],
            a2: [val; 32],
        }
    }
}

fn insert(c: &mut Criterion) {
    c.bench_function("HAMT bulk insert (no flush)", |b| {
        b.iter(|| {
            let db = fvm_ipld_blockstore::MemoryBlockstore::default();
            let mut a = Hamt::<_, _>::new_with_bit_width(&db, 5);

            for i in 0..black_box(ITEM_COUNT) {
                a.set(black_box(vec![i; 20].into()), black_box(BenchData::new(i)))
                    .unwrap();
            }
        })
    });
}

fn insert_load_flush(c: &mut Criterion) {
    c.bench_function("HAMT bulk insert with flushing and loading", |b| {
        b.iter(|| {
            let db = fvm_ipld_blockstore::MemoryBlockstore::default();
            let mut empt = Hamt::<_, ()>::new_with_bit_width(&db, 5);
            let mut cid = empt.flush().unwrap();

            for i in 0..black_box(ITEM_COUNT) {
                let mut a = Hamt::<_, _>::load_with_bit_width(&cid, &db, 5).unwrap();
                a.set(black_box(vec![i; 20].into()), black_box(BenchData::new(i)))
                    .unwrap();
                cid = a.flush().unwrap();
            }
        })
    });
}

fn delete(c: &mut Criterion) {
    let db = fvm_ipld_blockstore::MemoryBlockstore::default();
    let mut a = Hamt::<_, _>::new_with_bit_width(&db, 5);
    for i in 0..black_box(ITEM_COUNT) {
        a.set(vec![i; 20].into(), BenchData::new(i)).unwrap();
    }
    let cid = a.flush().unwrap();

    c.bench_function("HAMT deleting all nodes", |b| {
        b.iter(|| {
            let mut a = Hamt::<_, BenchData>::load_with_bit_width(&cid, &db, 5).unwrap();
            for i in 0..black_box(ITEM_COUNT) {
                a.delete(black_box([i; 20].as_ref())).unwrap();
            }
        })
    });
}

fn for_each(c: &mut Criterion) {
    let db = fvm_ipld_blockstore::MemoryBlockstore::default();
    let mut a = Hamt::<_, _>::new_with_bit_width(&db, 5);
    for i in 0..black_box(ITEM_COUNT) {
        a.set(vec![i; 20].into(), BenchData::new(i)).unwrap();
    }
    let cid = a.flush().unwrap();

    c.bench_function("HAMT for_each function", |b| {
        b.iter(|| {
            let a = Hamt::<_, _>::load_with_bit_width(&cid, &db, 5).unwrap();
            black_box(a).for_each(|_k, _v: &BenchData| Ok(())).unwrap();
        })
    });
}

criterion_group!(benches, insert, insert_load_flush, delete, for_each);
criterion_main!(benches);
