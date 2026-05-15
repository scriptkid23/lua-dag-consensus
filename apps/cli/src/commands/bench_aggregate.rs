//! Ad-hoc BLS aggregate throughput.

use std::time::Instant;

use anyhow::{Context, Result};
use crypto::{
    bls::{SecretKey, aggregate_sigs, sign},
    hash::dst,
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::args::BenchArgs;

/// Entrypoint.
pub fn run(args: &BenchArgs) -> Result<()> {
    let mut rng = ChaCha20Rng::from_seed([42; 32]);
    let mut sks = Vec::with_capacity(args.partials as usize);
    for _ in 0..args.partials {
        sks.push(SecretKey::random(&mut rng).context("BLS keygen")?);
    }
    let msg = b"bench-aggregate-message";
    let sigs: Vec<_> = sks.iter().map(|sk| sign(sk, dst::MICRO_QC, msg)).collect();

    let mut total_ns = 0u128;
    for _ in 0..args.iters {
        let t0 = Instant::now();
        let _agg = aggregate_sigs(&sigs)?;
        total_ns += t0.elapsed().as_nanos();
    }
    let avg_ns = total_ns / u128::from(args.iters);
    let aggs_per_sec = 1_000_000_000u128 / avg_ns.max(1);
    println!(
        "partials={} iters={} avg_ns={} aggs_per_sec={}",
        args.partials, args.iters, avg_ns, aggs_per_sec
    );
    Ok(())
}
