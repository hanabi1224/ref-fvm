// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::collections::HashMap;
use std::env::var;
use std::fs::File;
use std::io::BufReader;
use std::iter;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::thread::available_parallelism;

use anyhow::{anyhow, Context as _};
use async_std::{stream, sync, task};
use colored::*;
use futures::{Future, StreamExt, TryFutureExt, TryStreamExt};
use fvm::engine::MultiEngine;
use fvm_conformance_tests::driver::*;
use fvm_conformance_tests::report;
use fvm_conformance_tests::tracing::{TestTraceExporter, TestTraceExporterRef};
use fvm_conformance_tests::vector::{MessageVector, Selector};
use fvm_conformance_tests::vm::{TestStatsGlobal, TestStatsRef};
use itertools::Itertools;
use lazy_static::lazy_static;
use walkdir::WalkDir;

enum ErrorAction {
    Error,
    Warn,
    Ignore,
}

impl FromStr for ErrorAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "ignore" => Ok(Self::Ignore),
            _ => Err("must be one of error|warn|ignore".into()),
        }
    }
}

lazy_static! {
    /// The maximum parallelism when processing test vectors. Capped at 48.
    static ref TEST_VECTOR_PARALLELISM: usize = std::env::var_os("TEST_VECTOR_PARALLELISM")
        .map(|s| {
            let s = s.to_str().unwrap();
            s.parse().expect("parallelism must be an integer")
        }).or_else(|| available_parallelism().map(Into::into).ok()).unwrap_or(8).min(48);

    /// By default a post-condition error is fatal and stops all testing. We can use this env var to relax that
    /// and let the test carry on (optionally with a warning); there's a correctness check against the post condition anyway.
    static ref TEST_VECTOR_POSTCONDITION_MISSING_ACTION: ErrorAction = std::env::var_os("TEST_VECTOR_POSTCONDITION_MISSING_ACTION")
        .map(|s| {
            let s = s.to_str().unwrap();
            s.parse().expect("unexpected post condition error action")
        }).unwrap_or(ErrorAction::Warn);

    static ref ENGINES: MultiEngine = MultiEngine::new(*TEST_VECTOR_PARALLELISM as u32);
}

#[async_std::test]
async fn conformance_test_runner() -> anyhow::Result<()> {
    env_logger::init();

    let path = var("VECTOR").unwrap_or_else(|_| "test-vectors/corpus".to_owned());
    let path = Path::new(path.as_str()).to_path_buf();
    let stats = TestStatsGlobal::new_ref();

    // Optionally create a component to export gas charge traces.
    let tracer = std::env::var("TRACE_DIR")
        .ok()
        .map(|path| TestTraceExporter::new(Path::new(path.as_str()).to_path_buf()));

    let vector_results = if path.is_file() {
        let stats = stats.clone();
        let tracer = tracer.clone();
        either::Either::Left(
            iter::once(async move {
                let res = run_vector(path.clone(), stats, tracer)
                    .await
                    .with_context(|| format!("failed to run vector: {}", path.display()))?;
                anyhow::Ok((path, res))
            })
            .map(futures::future::Either::Left),
        )
    } else {
        either::Either::Right(
            WalkDir::new(path)
                .into_iter()
                .filter_ok(is_runnable)
                .map(|e| {
                    let stats = stats.clone();
                    let tracer = tracer.clone();
                    async move {
                        let path = e?.path().to_path_buf();
                        let res = run_vector(path.clone(), stats, tracer)
                            .await
                            .with_context(|| format!("failed to run vector: {}", path.display()))?;
                        Ok((path, res))
                    }
                })
                .map(futures::future::Either::Right),
        )
    };

    let mut results = Box::pin(
        stream::from_iter(vector_results)
            // Will _load_ up to 100 vectors at once in any order. We won't actually run the vectors in
            // parallel (yet), but that shouldn't be too hard.
            .map(|task| {
                async move {
                    let (path, jobs) = task.await?;
                    Ok(stream::from_iter(jobs).map(move |job| {
                        let path = path.clone();
                        Ok(async move { anyhow::Ok((path, job.await?)) })
                    }))
                }
                .try_flatten_stream()
            })
            .flatten()
            .try_buffer_unordered(*TEST_VECTOR_PARALLELISM),
    );

    let mut succeeded = 0;
    let mut failed = 0;
    let mut skipped = 0;

    while let Some((path, res)) = results.next().await.transpose()? {
        match res {
            VariantResult::Ok { id } => {
                report!("OK".on_green(), path.display(), id);
                succeeded += 1;
            }
            VariantResult::Failed { reason, id } => {
                report!("FAIL".white().on_red(), path.display(), id);
                println!("\t|> reason: {:#}", reason);
                failed += 1;
            }
            VariantResult::Skipped { reason, id } => {
                report!("SKIP".on_yellow(), path.display(), id);
                println!("\t|> reason: {}", reason);
                skipped += 1;
            }
        }
    }

    println!();
    println!(
        "{}",
        format!(
            "conformance tests result: {}/{} tests passed ({} skipped)",
            succeeded,
            failed + succeeded,
            skipped,
        )
        .bold()
    );

    if let Some(ref stats) = stats {
        let stats = stats.lock().unwrap();
        println!(
            "{}",
            format!(
                "memory stats:\n init.min: {}\n init.max: {}\n exec.min: {}\n exec.max: {}\n",
                stats.init.min_instance_memory_bytes,
                stats.init.max_instance_memory_bytes,
                stats.exec.min_instance_memory_bytes,
                stats.exec.max_instance_memory_bytes,
            )
            .bold()
        );
    }

    if let Some(ref tracer) = tracer {
        tracer.export_tombstones()?;
    }

    if failed > 0 {
        Err(anyhow!("some vectors failed"))
    } else {
        Ok(())
    }
}

/// Runs a single test vector and returns a list of VectorResults,
/// one per variant.
async fn run_vector(
    path: PathBuf,
    stats: TestStatsRef,
    tracer: TestTraceExporterRef,
) -> anyhow::Result<impl Iterator<Item = impl Future<Output = anyhow::Result<VariantResult>>>> {
    let file = File::open(&path)?;
    let reader = BufReader::new(file);

    // Test vectors have the form:
    //
    //     { "class": ..., rest... }
    //
    // Unfortunately:
    // 1. That means we need to use serde's "flatten" and/or "tag" feature to decode them.
    // 2. Serde's JSON library doesn't support arbitrary precision numbers when doing this.
    // 3. The circulating supply doesn't fit in a u64, and f64 isn't precise enough.
    //
    // So we manually:
    // 1. Decode into a map of `String` -> `raw data`.
    // 2. Pull off the class.
    // 3. Re-serialize.
    // 4. Decode into the correct type.
    //
    // Upstream bug is https://github.com/serde-rs/serde/issues/1183 (or at least that looks like
    // the most appropriate one out of all the related issues).
    let mut vector: HashMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_reader(reader).context("failed to parse vector")?;
    let class_json = vector
        .remove("class")
        .context("expected test vector to have a class")?;

    let class: &str =
        serde_json::from_str(class_json.get()).context("failed to parse test vector class")?;
    let vector_json = serde_json::to_string(&vector)?;

    match class {
        "message" => {
            let v: MessageVector =
                serde_json::from_str(&vector_json).context("failed to parse message vector")?;
            let skip = !v.selector.as_ref().is_none_or(Selector::supported);
            if skip {
                Ok(either::Either::Left(
                    v.preconditions.variants.into_iter().map(|variant| {
                        futures::future::Either::Left(async move {
                            Ok(VariantResult::Skipped {
                                id: variant.id,
                                reason: "selector not supported".to_owned(),
                            })
                        })
                    }),
                ))
            } else {
                // First import the blockstore and do some sanity checks.
                let (bs, imported_root) = v.seed_blockstore().await?;
                if !imported_root.contains(&v.preconditions.state_tree.root_cid) {
                    return Err(anyhow!(
                        "imported roots ({}) do not contain precondition CID {}",
                        imported_root.iter().join(", "),
                        v.preconditions.state_tree.root_cid
                    ));
                }
                if !imported_root.contains(&v.postconditions.state_tree.root_cid) {
                    let msg = format!(
                        "imported roots ({}) do not contain postcondition CID {}",
                        imported_root.iter().join(", "),
                        v.postconditions.state_tree.root_cid
                    );

                    match *TEST_VECTOR_POSTCONDITION_MISSING_ACTION {
                        ErrorAction::Error => {
                            return Err(anyhow!(msg));
                        }
                        ErrorAction::Warn => {
                            eprintln!("WARN: {msg} in {}", path.display())
                        }
                        ErrorAction::Ignore => (),
                    }
                }

                let v = sync::Arc::new(v);
                Ok(either::Either::Right(
                    (0..v.preconditions.variants.len()).map(move |i| {
                        let v = v.clone();
                        let bs = bs.clone();
                        let path = path.clone();
                        let variant_id = v.preconditions.variants[i].id.clone();
                        let name = format!("{} | {}", path.display(), variant_id);
                        let stats = stats.clone();
                        let tracer = tracer.clone();
                        futures::future::Either::Right(
                            task::Builder::new()
                                .name(name.clone())
                                .spawn(async move {
                                    run_variant(
                                        bs,
                                        &v,
                                        &v.preconditions.variants[i],
                                        &ENGINES,
                                        true,
                                        stats,
                                        tracer.map(|t| t.export_fun(path, variant_id)),
                                    )
                                    .with_context(|| format!("failed to run {name}"))
                                })
                                .unwrap(),
                        )
                    }),
                ))
            }
        }
        other => Err(anyhow!("unknown test vector class: {}", other)),
    }
}
