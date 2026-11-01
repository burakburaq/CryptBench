use crate::BenchResult;
use serde::Deserialize;
use sha2::Digest;
use std::{
    fs::OpenOptions,
    hint::black_box,
    io::Write,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(skip)]
    pub input: crate::input::InputConfig,
    pub workers_values: Vec<u32>,
    pub warmup: u32,
    pub duration_seconds: u32,
    #[serde(default = "crate::default_repeats")]
    pub repeats: u32,
    #[serde(default = "default_batch")]
    pub batch_size: u32,
}
fn default_batch() -> u32 {
    1024
}

fn digest<D: Digest>(input: &[u8]) {
    black_box(D::digest(black_box(input)));
}

fn benchmark<F: Fn(&[u8]) + Copy + Send + Sync>(
    workers: u32,
    c: &Config,
    hash: F,
    input: &[u8],
) -> BenchResult<(Vec<f64>, u64, f64, Option<u64>)> {
    thread::scope(|scope| -> BenchResult<_> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut starters = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..workers {
            let ready = ready_tx.clone();
            let (tx, rx) = mpsc::channel::<Instant>();
            starters.push(tx);
            handles.push(thread::Builder::new().spawn_scoped(
                scope,
                move || -> Result<_, String> {
                    for _ in 0..c.warmup {
                        hash(input);
                    }
                    ready.send(()).map_err(|e| e.to_string())?;
                    let start = rx.recv().map_err(|e| e.to_string())?;
                    let mut samples = Vec::new();
                    let mut count = 0u64;
                    let cycles_start = crate::cycles::current_thread();
                    while start.elapsed() < Duration::from_secs(u64::from(c.duration_seconds)) {
                        let batch_start = Instant::now();
                        for _ in 0..c.batch_size {
                            hash(input);
                        }
                        let ns =
                            batch_start.elapsed().as_secs_f64() * 1e9 / f64::from(c.batch_size);
                        samples.push(ns);
                        count += u64::from(c.batch_size);
                    }
                    let cycles =
                        crate::cycles::delta(cycles_start, crate::cycles::current_thread());
                    Ok((samples, count, start.elapsed().as_secs_f64(), cycles))
                },
            )?);
        }
        drop(ready_tx);
        for _ in 0..workers {
            ready_rx.recv()?;
        }
        let start = Instant::now();
        for tx in &starters {
            tx.send(start)?;
        }
        drop(starters);
        let mut samples = Vec::new();
        let mut count = 0;
        let mut elapsed = 0.0_f64;
        let mut total_cycles = Some(0u64);
        for handle in handles {
            let (mut values, n, seconds, cycles) =
                handle.join().map_err(|_| "SHA çalışanı durdu")??;
            samples.append(&mut values);
            count += n;
            total_cycles =
                total_cycles.and_then(|total| cycles.and_then(|value| total.checked_add(value)));
            elapsed = elapsed.max(seconds);
        }
        if samples.is_empty() {
            return Err("SHA ölçümünde işlem tamamlanamadı".into());
        }
        Ok((samples, count, elapsed, total_cycles))
    })
}

pub fn run<D: Digest + Send>(c: Config, name: &str) -> BenchResult<()> {
    run_fast(c, name, <D as Digest>::output_size(), digest::<D>)
}

pub fn run_blake3(c: Config) -> BenchResult<()> {
    run_fast(c, "BLAKE3", 32, |input| {
        black_box(blake3::hash(black_box(input)));
    })
}

fn run_fast<F: Fn(&[u8]) + Copy + Send + Sync>(
    mut c: Config,
    name: &str,
    output_size: usize,
    hash: F,
) -> BenchResult<()> {
    if c.workers_values.is_empty()
        || c.workers_values.contains(&0)
        || c.duration_seconds == 0
        || c.repeats == 0
        || c.batch_size == 0
    {
        return Err(
            "SHA: çalışan, süre, tekrar ve batch_size pozitif olmalı; çalışan listesi boş olamaz"
                .into(),
        );
    }
    let input = c.input.prepare()?;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    std::fs::create_dir_all(&dir)?;
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results.txt"))?;
    writeln!(
        log,
        "\nTarih/Saat: {} | {} {}",
        chrono::Local::now().format("%d.%m.%Y-%H:%M:%S"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )?;
    writeln!(log, "{name} | input_group=hashing input={} B mode={} hash={} B | batch_size={} | süreler=batch ortalaması/hash (ns)", input.bytes.len(), input.mode, output_size, c.batch_size)?;
    let header = format!(
        "{:>3} {:>7} {:>12} {:>8} {:>10} {:>10} {:>10} {:>10} {:>10} {:>14} {:>10}",
        "Tur",
        "Workers",
        "Hash",
        "Süre(sn)",
        "Ort.ns",
        "Medyan",
        "p95",
        "Min",
        "Maks",
        "hash/sn",
        "MiB/sn"
    );
    crate::console::start(
        name,
        false,
        false,
        false,
        true,
        (c.duration_seconds, c.warmup, c.repeats, Some(c.batch_size)),
    );
    let mut console_rows = Vec::new();
    writeln!(log, "{header}")?;
    log.flush()?;
    c.workers_values.sort_unstable();
    for &workers in &c.workers_values {
        for round in 1..=c.repeats {
            let loading = crate::console::Loading::start();
            let result = benchmark(workers, &c, hash, &input.bytes);
            drop(loading);
            let (mut samples, count, elapsed, cycles) = result?;
            let cycles_per_byte = crate::cycles::per_byte(cycles, count, input.bytes.len());
            samples.sort_by(f64::total_cmp);
            let n = samples.len();
            let mean = samples.iter().sum::<f64>() / n as f64;
            let median = (samples[(n - 1) / 2] + samples[n / 2]) / 2.0;
            let p95 = samples[(n as f64 * 0.95).ceil() as usize - 1];
            let rate = count as f64 / elapsed;
            let line = format!("{:>3} {:>7} {:>12} {:>8.3} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>14.2} {:>10.2}", round, workers, count, elapsed, mean, median, p95, samples[0], samples[n-1], rate, rate * input.bytes.len() as f64 / 1048576.0);
            crate::results::save_json(
                &name.to_lowercase().replace('-', ""),
                serde_json::json!({
                    "status": "success", "round": round,
                    "input_group": "hashing", "input_mode": input.mode, "input_bytes": input.bytes.len(),
                    "salt_bytes": 0, "hash_bytes": output_size,
                    "parameters": { "workers": workers, "warmup_per_worker": c.warmup, "duration_seconds": c.duration_seconds, "repeats": c.repeats, "batch_size": c.batch_size },
                    "hash_count": count, "elapsed_seconds": elapsed, "cycles_per_byte": cycles_per_byte, "thread_cycles": cycles, "cycle_counter": "windows_query_thread_cycle_time",
                    "hashes_per_second": rate, "mib_per_second": rate * input.bytes.len() as f64 / 1048576.0,
                    "timing": { "unit": "ns", "measurement": "batch_average_per_hash", "batch_count": n, "mean": mean, "median": median, "p95": p95, "min": samples[0], "max": samples[n-1] }
                }),
            )?;
            console_rows.push(crate::console::Row {
                algorithm: name.to_owned(),
                has_cycles: true,
                cycles_per_byte,
                argon: None,
                scrypt: None,
                group: String::new(),
                workers,
                round,
                operation: None,
                avg_ns: mean,
                median_ns: median,
                p95_ns: p95,
                min_ns: samples[0],
                max_ns: samples[n - 1],
                rate,
                mib: Some(rate * input.bytes.len() as f64 / 1048576.0),
            });
            crate::console::row(&console_rows, false);
            writeln!(
                log,
                "{line}   cycles/B={}",
                cycles_per_byte
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "-".into())
            )?;
            log.flush()?;
        }
    }
    crate::console::show(name, &console_rows, false);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Sha256, Sha384, Sha512};
    #[test]
    fn sha3_and_blake3_known_vectors() {
        assert_eq!(
            format!("{:x}", sha3::Sha3_256::digest(b"abc")),
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
        assert_eq!(
            blake3::hash(b"").to_hex().as_str(),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }
    #[test]
    fn sha_known_vectors() {
        assert_eq!(
            format!("{:x}", Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(format!("{:x}", Sha384::digest(b"abc")), "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7");
        assert_eq!(format!("{:x}", Sha512::digest(b"abc")), "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f");
    }
}
