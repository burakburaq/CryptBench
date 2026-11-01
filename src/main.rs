mod cipher_bench;
mod console;
mod cycles;
mod input;
mod results;
mod scrypt_bench;
mod sha;
use argon2::{Algorithm, Argon2, Block, Params, Version};
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use std::{
    error::Error,
    fs::{File, OpenOptions},
    hint::black_box,
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

type BenchResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, ValueEnum)]
enum AlgorithmChoice {
    Argon2id,
    Sha2,
    Sha3,
    Sha256,
    Sha384,
    Sha512,
    Sha3_256,
    Sha3_384,
    Sha3_512,
    Blake3,
    Scrypt,
    Aes256gcm,
    Chacha20poly1305,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputGroups {
    #[serde(default)]
    password_hashing: input::InputConfig,
    #[serde(default)]
    hashing: input::InputConfig,
    #[serde(default)]
    encryption: input::InputConfig,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootConfig {
    #[serde(default)]
    inputs: InputGroups,
    argon2id: Config,
    sha2: ShaFamily,
    sha3: ShaFamily,
    blake3: sha::Config,
    scrypt: scrypt_bench::Config,
    #[serde(default)]
    aes256gcm: cipher_bench::Config,
    #[serde(default)]
    chacha20poly1305: cipher_bench::Config,
}

fn load_config(path: &Path, preset_override: Option<&Path>) -> BenchResult<RootConfig> {
    let mut config: serde_yaml::Value = serde_json::from_reader(File::open(path)?)?;
    let fields = config
        .as_mapping_mut()
        .ok_or("Configuration must be a JSON object")?;
    let workers = fields
        .remove(serde_yaml::Value::from("workers_values"))
        .ok_or("Configuration must specify workers_values")?;
    let worker_values: Vec<u32> = serde_yaml::from_value(workers.clone())?;
    if worker_values.is_empty() || worker_values.contains(&0) {
        return Err("workers_values must be a nonempty list of positive integers".into());
    }
    let preset_key = serde_yaml::Value::from("preset");
    let configured_preset = fields.remove(&preset_key);
    let preset = match (preset_override, configured_preset) {
        (Some(path), _) => Some(path.to_path_buf()),
        (None, Some(value)) => {
            let name = value.as_str().ok_or("preset must be a path string")?;
            Some(path.parent().unwrap_or_else(|| Path::new(".")).join(name))
        }
        (None, None) => None,
    };
    let preset_name = preset
        .as_deref()
        .and_then(Path::file_stem)
        .map(|name| name.to_string_lossy().into_owned());
    if let Some(preset) = preset {
        let preset_fields: serde_yaml::Mapping = serde_yaml::from_reader(
            File::open(&preset)
                .map_err(|e| format!("Cannot open preset {}: {e}", preset.display()))?,
        )?;
        for (key, value) in preset_fields {
            if key.as_str() == Some("inputs") || key.as_str() == Some("preset") {
                return Err("A preset cannot define inputs or another preset".into());
            }
            if fields.contains_key(&key) {
                return Err(format!(
                    "Duplicate configuration field: {}",
                    key.as_str().unwrap_or("<non-string>")
                )
                .into());
            }
            fields.insert(key, value);
        }
    }
    for algorithm in [
        "argon2id",
        "scrypt",
        "blake3",
        "sha2",
        "sha3",
        "aes256gcm",
        "chacha20poly1305",
    ] {
        if let Some(section) = fields.get_mut(serde_yaml::Value::from(algorithm)) {
            let section = section
                .as_mapping_mut()
                .ok_or_else(|| format!("{algorithm} must be a mapping"))?;
            if section.contains_key(serde_yaml::Value::from("workers_values")) {
                return Err(format!(
                    "{algorithm}.workers_values must be set globally in config.json"
                )
                .into());
            }
            section.insert(serde_yaml::Value::from("workers_values"), workers.clone());
        }
    }
    let config = serde_yaml::from_value(config)?;
    console::set_preset(preset_name);
    Ok(config)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShaFamily {
    variants: Vec<u16>,
    workers_values: Vec<u32>,
    warmup: u32,
    duration_seconds: u32,
    #[serde(default = "default_repeats")]
    repeats: u32,
    #[serde(default = "sha_family_batch")]
    batch_size: u32,
}
fn sha_family_batch() -> u32 {
    1024
}
impl ShaFamily {
    fn validate(&self) -> BenchResult<()> {
        let mut seen = std::collections::HashSet::new();
        if self.variants.is_empty()
            || self
                .variants
                .iter()
                .any(|v| ![256, 384, 512].contains(v) || !seen.insert(*v))
        {
            return Err(
                "SHA variants must be a nonempty list of distinct values 256, 384, 512".into(),
            );
        }
        Ok(())
    }
    fn settings(&self, input: input::InputConfig) -> sha::Config {
        sha::Config {
            input,
            workers_values: self.workers_values.clone(),
            warmup: self.warmup,
            duration_seconds: self.duration_seconds,
            repeats: self.repeats,
            batch_size: self.batch_size,
        }
    }
}

#[derive(Parser)]
#[command(about = "CryptBench: hash and authenticated encryption benchmark")]
struct Cli {
    #[arg(long, value_enum, default_value = "argon2id")]
    algorithm: AlgorithmChoice,
    #[arg(long, conflicts_with_all = ["m", "t", "p", "profiles", "warmup", "duration", "workers", "repeats"])]
    config: Option<PathBuf>,
    /// Override the preset specified in the configuration file.
    #[arg(long, requires = "config")]
    preset: Option<PathBuf>,
    /// Bellek (KiB)
    #[arg(long, default_value_t = 19456, conflicts_with = "profiles")]
    m: u32,
    #[arg(long, default_value_t = 2, conflicts_with = "profiles")]
    t: u32,
    #[arg(long, default_value_t = 1, conflicts_with = "profiles")]
    p: u32,
    #[arg(long)]
    profiles: bool,
    #[arg(long, default_value_t = 10)]
    warmup: u32,
    #[arg(long, default_value_t = 5)]
    duration: u32,
    #[arg(long, default_value_t = 1)]
    workers: u32,
    #[arg(long, default_value_t = 1)]
    repeats: u32,
}

// Önceden kullanılan tek değerleri de kabul et.
#[derive(Deserialize)]
#[serde(untagged)]
enum Values {
    One(u32),
    Many(Vec<u32>),
}
impl Values {
    fn list(self) -> Vec<u32> {
        match self {
            Self::One(v) => vec![v],
            Self::Many(v) => v,
        }
    }
}
fn default_workers() -> Vec<u32> {
    vec![1]
}
fn default_repeats() -> u32 {
    1
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(skip)]
    input: input::InputConfig,
    m_mb: Values,
    t: Values,
    p_values: Vec<u32>,
    #[serde(default = "default_workers")]
    workers_values: Vec<u32>,
    warmup: u32,
    duration_seconds: u32,
    #[serde(default = "default_repeats")]
    repeats: u32,
}

#[derive(Clone)]
struct Profile {
    m: u32,
    t: u32,
    p: u32,
    workers: u32,
}

fn hash(argon: &Argon2<'_>, memory: &mut [Block], input: &[u8]) -> Result<f64, String> {
    let mut salt = [0; 16];
    getrandom::getrandom(&mut salt).map_err(|e| e.to_string())?;
    let mut output = [0; 32];
    let start = Instant::now();
    argon
        .hash_password_into_with_memory(black_box(input), &salt, &mut output, memory)
        .map_err(|e| e.to_string())?;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    black_box(output);
    Ok(ms)
}

fn benchmark(
    profile: &Profile,
    warmup: u32,
    duration: u32,
    input: &[u8],
) -> BenchResult<(Vec<f64>, f64)> {
    let params =
        Params::new(profile.m, profile.t, profile.p, Some(32)).map_err(|e| e.to_string())?;
    thread::scope(|scope| -> BenchResult<_> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut starters = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..profile.workers {
            let ready = ready_tx.clone();
            let (start_tx, start_rx) = mpsc::channel::<Instant>();
            starters.push(start_tx);
            let params = params.clone();
            let handle =
                thread::Builder::new().spawn_scoped(scope, move || -> Result<_, String> {
                    let mut memory = Vec::new();
                    let prepared = (|| -> Result<_, String> {
                        memory
                            .try_reserve_exact(params.block_count())
                            .map_err(|e| e.to_string())?;
                        memory.resize(params.block_count(), Block::default());
                        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
                        for _ in 0..warmup {
                            hash(&argon, &mut memory, input)?;
                        }
                        Ok(argon)
                    })();
                    ready
                        .send(prepared.as_ref().map(|_| ()).map_err(Clone::clone))
                        .map_err(|e| e.to_string())?;
                    let argon = prepared?;
                    let start = start_rx.recv().map_err(|e| e.to_string())?;
                    let mut samples = Vec::new();
                    while start.elapsed() < Duration::from_secs(u64::from(duration)) {
                        samples.push(hash(&argon, &mut memory, input)?);
                    }
                    // Tamponun bırakılması ölçüm süresine dahil edilmez.
                    Ok((samples, start.elapsed().as_secs_f64()))
                })?;
            handles.push(handle);
        }
        drop(ready_tx);
        for _ in 0..profile.workers {
            ready_rx
                .recv()?
                .map_err(|e| format!("Çalışan hazırlanamadı: {e}"))?;
        }
        let start = Instant::now();
        for tx in &starters {
            tx.send(start)?;
        }
        drop(starters);
        let mut samples = Vec::new();
        let mut elapsed = 0.0_f64;
        for handle in handles {
            let (mut values, seconds) = handle
                .join()
                .map_err(|_| "Çalışan beklenmedik şekilde durdu")??;
            samples.append(&mut values);
            elapsed = elapsed.max(seconds);
        }
        if samples.is_empty() {
            return Err("Ölçüm süresinde hiçbir hash tamamlanamadı".into());
        }
        Ok((samples, elapsed))
    })
}

fn main() {
    let result = run().and_then(|()| results::finish());
    if let Err(error) = &result {
        console::error(&error.to_string());
    }
    results::print_saved();
    if result.is_err() {
        std::process::exit(1);
    }
}

fn run() -> BenchResult<()> {
    let cli = Cli::parse();
    results::begin(
        cli.algorithm
            .to_possible_value()
            .expect("algorithm name")
            .get_name(),
    );
    if matches!(
        cli.algorithm,
        AlgorithmChoice::Aes256gcm | AlgorithmChoice::Chacha20poly1305
    ) {
        if cli.profiles || cli.m != 19456 || cli.t != 2 || cli.p != 1 {
            return Err("Cipher benchmarks do not use m, t, p or profiles".into());
        }
        let (config, input) = if let Some(path) = cli.config {
            let root: RootConfig = load_config(&path, cli.preset.as_deref())?;
            let config = match cli.algorithm {
                AlgorithmChoice::Aes256gcm => root.aes256gcm,
                _ => root.chacha20poly1305,
            };
            (config, root.inputs.encryption)
        } else {
            (
                cipher_bench::Config::defaults(cli.workers, cli.warmup, cli.duration, cli.repeats),
                input::InputConfig::default(),
            )
        };
        return match cli.algorithm {
            AlgorithmChoice::Aes256gcm => {
                cipher_bench::run::<aes_gcm::Aes256Gcm>(config, input, "AES-256-GCM", "aes256gcm")
            }
            AlgorithmChoice::Chacha20poly1305 => {
                cipher_bench::run::<chacha20poly1305::ChaCha20Poly1305>(
                    config,
                    input,
                    "ChaCha20-Poly1305",
                    "chacha20poly1305",
                )
            }
            _ => unreachable!(),
        };
    }
    if matches!(cli.algorithm, AlgorithmChoice::Scrypt) {
        if cli.profiles || cli.m != 19456 || cli.t != 2 || cli.p != 1 {
            return Err("scrypt uses its own parameters in config.json".into());
        }
        let config = if let Some(path) = cli.config {
            {
                let root: RootConfig = load_config(&path, cli.preset.as_deref())?;
                let mut config = root.scrypt;
                config.input = root.inputs.password_hashing;
                config
            }
        } else {
            scrypt_bench::Config::defaults(cli.workers, cli.warmup, cli.duration, cli.repeats)
        };
        return scrypt_bench::run(config);
    }
    if !matches!(cli.algorithm, AlgorithmChoice::Argon2id) {
        if cli.profiles || cli.m != 19456 || cli.t != 2 || cli.p != 1 {
            return Err("SHA/BLAKE3 do not use m, t, p or profiles".into());
        }
        let (family, input, blake) = if let Some(path) = cli.config {
            let root: RootConfig = load_config(&path, cli.preset.as_deref())?;
            let is_sha3 = matches!(
                cli.algorithm,
                AlgorithmChoice::Sha3
                    | AlgorithmChoice::Sha3_256
                    | AlgorithmChoice::Sha3_384
                    | AlgorithmChoice::Sha3_512
            );
            let family = if is_sha3 { root.sha3 } else { root.sha2 };
            (family, root.inputs.hashing, Some(root.blake3))
        } else {
            (
                ShaFamily {
                    variants: vec![256, 384, 512],
                    workers_values: vec![cli.workers],
                    warmup: cli.warmup,
                    duration_seconds: cli.duration,
                    repeats: cli.repeats,
                    batch_size: 1024,
                },
                input::InputConfig::default(),
                None,
            )
        };
        if matches!(cli.algorithm, AlgorithmChoice::Blake3) {
            let mut config =
                blake.unwrap_or_else(|| family.settings(input::InputConfig::default()));
            config.input = input;
            return sha::run_blake3(config);
        }
        family.validate()?;
        let selected: Vec<u16> = match cli.algorithm {
            AlgorithmChoice::Sha2 | AlgorithmChoice::Sha3 => family.variants.clone(),
            AlgorithmChoice::Sha256 | AlgorithmChoice::Sha3_256 => vec![256],
            AlgorithmChoice::Sha384 | AlgorithmChoice::Sha3_384 => vec![384],
            AlgorithmChoice::Sha512 | AlgorithmChoice::Sha3_512 => vec![512],
            _ => unreachable!(),
        };
        let is_sha3 = matches!(
            cli.algorithm,
            AlgorithmChoice::Sha3
                | AlgorithmChoice::Sha3_256
                | AlgorithmChoice::Sha3_384
                | AlgorithmChoice::Sha3_512
        );
        for variant in selected {
            let config = family.settings(input.clone());
            match (is_sha3, variant) {
                (false, 256) => sha::run::<sha2::Sha256>(config, "SHA-256")?,
                (false, 384) => sha::run::<sha2::Sha384>(config, "SHA-384")?,
                (false, 512) => sha::run::<sha2::Sha512>(config, "SHA-512")?,
                (true, 256) => sha::run::<sha3::Sha3_256>(config, "SHA3-256")?,
                (true, 384) => sha::run::<sha3::Sha3_384>(config, "SHA3-384")?,
                (true, 512) => sha::run::<sha3::Sha3_512>(config, "SHA3-512")?,
                _ => unreachable!(),
            }
        }
        return Ok(());
    }
    let (mut profiles, warmup, duration, repeats, input_config) = if let Some(path) = cli.config {
        let root: RootConfig = load_config(&path, cli.preset.as_deref())?;
        let mut c = root.argon2id;
        c.input = root.inputs.password_hashing;
        let mut profiles = Vec::new();
        let ts = c.t.list();
        let ms = c.m_mb.list();
        if ms.is_empty() || ts.is_empty() || c.p_values.is_empty() || c.workers_values.is_empty() {
            return Err("Parametre listeleri boş olamaz".into());
        }
        for m in ms {
            for &t in &ts {
                for &p in &c.p_values {
                    for &workers in &c.workers_values {
                        profiles.push(Profile {
                            m: m.checked_mul(1024).ok_or("m_mb çok büyük")?,
                            t,
                            p,
                            workers,
                        });
                    }
                }
            }
        }
        (profiles, c.warmup, c.duration_seconds, c.repeats, c.input)
    } else {
        let profiles = if cli.profiles {
            [(19456, 2), (65536, 3), (131072, 3)]
                .into_iter()
                .map(|(m, t)| Profile {
                    m,
                    t,
                    p: 1,
                    workers: cli.workers,
                })
                .collect()
        } else {
            vec![Profile {
                m: cli.m,
                t: cli.t,
                p: cli.p,
                workers: cli.workers,
            }]
        };
        (
            profiles,
            cli.warmup,
            cli.duration,
            cli.repeats,
            input::InputConfig::default(),
        )
    };
    if duration == 0 || repeats == 0 {
        return Err("Süre ve tekrar sayısı en az 1 olmalı".into());
    }
    for p in &profiles {
        if p.workers == 0 {
            return Err("Çalışan sayısı en az 1 olmalı".into());
        }
        Params::new(p.m, p.t, p.p, Some(32)).map_err(|e| format!("Geçersiz parametreler: {e}"))?;
    }
    let input = input_config.prepare()?;
    let results_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    std::fs::create_dir_all(&results_dir)?;
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
        "Argon2id v1.3 | input_group=password_hashing input={} B mode={} | salt=16 B hash=32 B",
        input.bytes.len(),
        input.mode
    )?;
    let header = format!(
        "{:>3} {:>8} {:>3} {:>3} {:>7} {:>8} {:>8} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "Tur",
        "m(MiB)",
        "t",
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
    console::set_argon_memory_profiles(profiles.iter().map(|profile| profile.m));
    console::start(
        "Argon2id",
        false,
        true,
        false,
        false,
        (duration, warmup, repeats, None),
    );
    let mut console_rows = Vec::new();
    writeln!(log, "{header}")?;
    log.flush()?;
    profiles.sort_by_key(|p| p.workers);
    for p in &profiles {
        for round in 1..=repeats {
            let loading = console::Loading::start();
            let result = benchmark(p, warmup, duration, &input.bytes);
            drop(loading);
            let (mut samples, elapsed) = result?;
            samples.sort_by(f64::total_cmp);
            let n = samples.len();
            let mean = samples.iter().sum::<f64>() / n as f64;
            let median = (samples[(n - 1) / 2] + samples[n / 2]) / 2.0;
            let p95 = samples[(n as f64 * 0.95).ceil() as usize - 1];
            let line = format!("{:>3} {:>8.2} {:>3} {:>3} {:>7} {:>8} {:>8.3} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2}", round, p.m as f64 / 1024.0, p.t, p.p, p.workers, n, elapsed, mean, median, p95, samples[0], samples[n-1], n as f64 / elapsed);
            results::save_json(
                "argon2id",
                serde_json::json!({
                    "status": "success", "round": round,
                    "input_group": "password_hashing", "input_mode": input.mode, "input_bytes": input.bytes.len(),
                    "argon2_version": "1.3", "salt_bytes": 16, "hash_bytes": 32,
                    "parameters": { "m_mib": p.m as f64 / 1024.0, "m_kib": p.m, "t": p.t, "p": p.p, "workers": p.workers, "warmup_per_worker": warmup, "duration_seconds": duration, "repeats": repeats },
                    "hash_count": n, "elapsed_seconds": elapsed,
                    "hashes_per_second": n as f64 / elapsed,
                    "timing": { "unit": "ms", "measurement": "individual_hash", "mean": mean, "median": median, "p95": p95, "min": samples[0], "max": samples[n-1] }
                }),
            )?;
            console_rows.push(console::Row {
                algorithm: "argon2id".into(),
                has_cycles: false,
                cycles_per_byte: None,
                argon: Some((p.m as f64 / 1024.0, p.t, p.p)),
                scrypt: None,
                group: format!("m={:.2} MiB t={} p={}", p.m as f64 / 1024.0, p.t, p.p),
                workers: p.workers,
                round,
                operation: None,
                avg_ns: mean * 1e6,
                median_ns: median * 1e6,
                p95_ns: p95 * 1e6,
                min_ns: samples[0] * 1e6,
                max_ns: samples[n - 1] * 1e6,
                rate: n as f64 / elapsed,
                mib: None,
            });
            console::row(&console_rows, false);
            writeln!(log, "{line}")?;
            log.flush()?;
        }
    }
    console::show("Argon2id", &console_rows, false);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reused_memory_matches_allocating_api() {
        for p in [1, 2, 4] {
            let params = Params::new(32, 2, p, Some(32)).unwrap();
            let mut memory = vec![Block::default(); params.block_count()];
            let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
            for salt in [[1u8; 16], [2u8; 16]] {
                let mut expected = [0; 32];
                let mut actual = [0; 32];
                argon
                    .hash_password_into(b"password", &salt, &mut expected)
                    .unwrap();
                argon
                    .hash_password_into_with_memory(b"password", &salt, &mut actual, &mut memory)
                    .unwrap();
                assert_eq!(actual, expected);
            }
        }
    }
}
