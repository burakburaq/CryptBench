use crate::BenchResult;
use scrypt::{scrypt, Params};
use serde::Deserialize;
use std::{
    fs::OpenOptions,
    hint::black_box,
    io::Write,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(skip)]
    pub input: crate::input::InputConfig,
    log_n_values: Vec<u8>,
    r_values: Vec<u32>,
    p_values: Vec<u32>,
    workers_values: Vec<u32>,
    warmup: u32,
    duration_seconds: u32,
    #[serde(default = "crate::default_repeats")]
    repeats: u32,
}
impl Config {
    pub fn defaults(workers: u32, warmup: u32, duration_seconds: u32, repeats: u32) -> Self {
        Self {
            input: crate::input::InputConfig::default(),
            log_n_values: vec![14],
            r_values: vec![8],
            p_values: vec![1],
            workers_values: vec![workers],
            warmup,
            duration_seconds,
            repeats,
        }
    }
}
#[derive(Clone)]
struct Profile {
    log_n: u8,
    r: u32,
    p: u32,
    workers: u32,
}
fn hash(params: &Params, input: &[u8]) -> Result<f64, String> {
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt).map_err(|e| e.to_string())?;
    let mut output = [0u8; 32];
    let start = Instant::now();
    scrypt(black_box(input), &salt, params, &mut output).map_err(|e| e.to_string())?;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    black_box(output);
    Ok(ms)
}
fn benchmark(profile: &Profile, c: &Config, input: &[u8]) -> BenchResult<(Vec<f64>, f64)> {
    let params = Params::new(profile.log_n, profile.r, profile.p, 32).map_err(|e| e.to_string())?;
    thread::scope(|scope| -> BenchResult<_> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut starters = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..profile.workers {
            let ready = ready_tx.clone();
            let (tx, rx) = mpsc::channel::<Instant>();
            starters.push(tx);
            let params = params.clone();
            handles.push(thread::Builder::new().spawn_scoped(
                scope,
                move || -> Result<_, String> {
                    let prepared = (|| -> Result<(), String> {
                        for _ in 0..c.warmup {
                            hash(&params, input)?;
                        }
                        Ok(())
                    })();
                    ready
                        .send(prepared.as_ref().map(|_| ()).map_err(Clone::clone))
                        .map_err(|e| e.to_string())?;
                    prepared?;
                    let start = rx.recv().map_err(|e| e.to_string())?;
                    let mut samples = Vec::new();
                    while start.elapsed() < Duration::from_secs(u64::from(c.duration_seconds)) {
                        samples.push(hash(&params, input)?);
                    }
                    Ok((samples, start.elapsed().as_secs_f64()))
                },
            )?);
        }
        drop(ready_tx);
        for _ in 0..profile.workers {
            ready_rx
                .recv()?
                .map_err(|e| format!("scrypt warmup failed: {e}"))?;
        }
        let start = Instant::now();
        for tx in &starters {
            tx.send(start)?;
        }
        drop(starters);
        let mut samples = Vec::new();
        let mut elapsed = 0.0_f64;
        for handle in handles {
            let (mut values, seconds) = handle.join().map_err(|_| "scrypt worker failed")??;
            samples.append(&mut values);
            elapsed = elapsed.max(seconds);
        }
        if samples.is_empty() {
            return Err("No scrypt hashes completed".into());
        }
        Ok((samples, elapsed))
    })
}
pub fn run(c: Config) -> BenchResult<()> {
    if c.log_n_values.is_empty()
        || c.r_values.is_empty()
        || c.p_values.is_empty()
        || c.workers_values.is_empty()
        || c.workers_values.contains(&0)
        || c.duration_seconds == 0
        || c.repeats == 0
    {
        return Err(
            "scrypt parameter lists, workers, duration and repeats must be nonempty and positive"
                .into(),
        );
    }
    let mut profiles = Vec::new();
    for &log_n in &c.log_n_values {
        for &r in &c.r_values {
            for &p in &c.p_values {
                for &workers in &c.workers_values {
                    Params::new(log_n, r, p, 32)
                        .map_err(|e| format!("Invalid scrypt parameters: {e}"))?;
                    profiles.push(Profile {
                        log_n,
                        r,
                        p,
                        workers,
                    });
                }
            }
        }
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
    writeln!(
        log,
        "scrypt | input_group=password_hashing input={} B mode={} | salt=16 B hash=32 B",
        input.bytes.len(),
        input.mode
    )?;
    let header = format!(
        "{:>3} {:>6} {:>5} {:>4} {:>7} {:>8} {:>8} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "Tur",
        "log2N",
        "r",
        "p",
        "Workers",
        "Hash",
        "Süre(sn)",
        "Ort.ms",
        "Medyan",
        "p95",
        "Min",
        "Maks",
        "hash/sn"
    );
    crate::console::start(
        "scrypt",
        false,
        false,
        true,
        false,
        (c.duration_seconds, c.warmup, c.repeats, None),
    );
    let mut console_rows = Vec::new();
    writeln!(log, "{header}")?;
    log.flush()?;
    profiles.sort_by_key(|p| p.workers);
    for p in &profiles {
        for round in 1..=c.repeats {
            let loading = crate::console::Loading::start();
            let result = benchmark(p, &c, &input.bytes);
            drop(loading);
            let (mut samples, elapsed) = result?;
            samples.sort_by(f64::total_cmp);
            let n = samples.len();
            let mean = samples.iter().sum::<f64>() / n as f64;
            let median = (samples[(n - 1) / 2] + samples[n / 2]) / 2.0;
            let p95 = samples[(n as f64 * 0.95).ceil() as usize - 1];
            let rate = n as f64 / elapsed;
            let line=format!("{:>3} {:>6} {:>5} {:>4} {:>7} {:>8} {:>8.3} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2}",round,p.log_n,p.r,p.p,p.workers,n,elapsed,mean,median,p95,samples[0],samples[n-1],rate);
            crate::results::save_json(
                "scrypt",
                serde_json::json!({"status":"success","round":round,"input_group":"password_hashing","input_mode":input.mode,"input_bytes":input.bytes.len(),"salt_bytes":16,"hash_bytes":32,"parameters":{"log_n":p.log_n,"n":1u64 << p.log_n,"r":p.r,"p":p.p,"workers":p.workers,"warmup_per_worker":c.warmup,"duration_seconds":c.duration_seconds,"repeats":c.repeats},"hash_count":n,"elapsed_seconds":elapsed,"hashes_per_second":rate,"timing":{"unit":"ms","measurement":"individual_hash","mean":mean,"median":median,"p95":p95,"min":samples[0],"max":samples[n-1]}}),
            )?;
            console_rows.push(crate::console::Row {
                algorithm: "scrypt".into(),
                has_cycles: false,
                cycles_per_byte: None,
                argon: None,
                scrypt: Some((p.log_n, p.r, p.p)),
                group: format!("log2N={} r={} p={}", p.log_n, p.r, p.p),
                workers: p.workers,
                round,
                operation: None,
                avg_ns: mean * 1e6,
                median_ns: median * 1e6,
                p95_ns: p95 * 1e6,
                min_ns: samples[0] * 1e6,
                max_ns: samples[n - 1] * 1e6,
                rate,
                mib: None,
            });
            crate::console::row(&console_rows, false);
            writeln!(log, "{line}")?;
            log.flush()?;
        }
    }
    crate::console::show("scrypt", &console_rows, false);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc7914_vector() {
        let mut out = [0u8; 64];
        scrypt(b"", b"", &Params::new(4, 1, 1, 64).unwrap(), &mut out).unwrap();
        assert_eq!(out.iter().map(|b|format!("{b:02x}")).collect::<String>(),"77d6576238657b203b19ca42c18a0497f16b4844e3074ae8dfdffa3fede21442fcd0069ded0948f8326a753a0fc81f17e8d3e0fb2e0d3628cf35e20c38d18906");
    }
}
