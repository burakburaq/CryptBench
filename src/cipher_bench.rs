use crate::{input::InputConfig, BenchResult};
use aes_gcm::aead::{
    consts::{U12, U16},
    AeadCore, AeadInPlace, KeyInit,
};
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

type Cipher<C> = C;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    workers_values: Vec<u32>,
    warmup: u32,
    duration_seconds: u32,
    #[serde(default = "crate::default_repeats")]
    repeats: u32,
    #[serde(default = "default_batch")]
    batch_size: u32,
}
fn default_batch() -> u32 {
    16
}
impl Default for Config {
    fn default() -> Self {
        Self::defaults(1, 10, 5, 1)
    }
}
impl Config {
    pub fn defaults(workers: u32, warmup: u32, duration_seconds: u32, repeats: u32) -> Self {
        Self {
            workers_values: vec![workers],
            warmup,
            duration_seconds,
            repeats,
            batch_size: 16,
        }
    }
}
#[derive(Clone, Copy)]
enum Operation {
    Encrypt,
    Decrypt,
}
impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::Encrypt => "encrypt",
            Self::Decrypt => "decrypt",
        }
    }
}

fn benchmark<C>(
    workers: u32,
    c: &Config,
    input: &[u8],
    operation: Operation,
) -> BenchResult<(Vec<f64>, u64, f64, Option<u64>)>
where
    C: AeadInPlace + KeyInit + AeadCore<NonceSize = U12, TagSize = U16> + Send,
{
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
                    let prepared = (|| -> Result<_, String> {
                        let mut key = [0u8; 32];
                        getrandom::getrandom(&mut key).map_err(|e| e.to_string())?;
                        let cipher =
                            Cipher::<C>::new_from_slice(&key).map_err(|_| "invalid key")?;
                        let mut fixture = input.to_vec();
                        let fixture_nonce = [0u8; 12];
                        let tag = cipher
                            .encrypt_in_place_detached((&fixture_nonce).into(), b"", &mut fixture)
                            .map_err(|_| "fixture encryption failed")?;
                        let mut buffer = Vec::new();
                        buffer
                            .try_reserve_exact(input.len())
                            .map_err(|e| e.to_string())?;
                        Ok((cipher, fixture, fixture_nonce, tag, buffer))
                    })();
                    let (cipher, fixture, fixture_nonce, tag, mut buffer) = match prepared {
                        Ok(value) => value,
                        Err(error) => {
                            let _ = ready.send(Err(error.clone()));
                            return Err(error);
                        }
                    };
                    let mut counter = 1u64;
                    let mut perform = || -> Result<(), String> {
                        buffer.clear();
                        match operation {
                            Operation::Encrypt => {
                                buffer.extend_from_slice(black_box(input));
                                let mut nonce = [0u8; 12];
                                nonce[4..].copy_from_slice(&counter.to_be_bytes());
                                counter =
                                    counter.checked_add(1).ok_or("nonce counter exhausted")?;
                                let tag = cipher
                                    .encrypt_in_place_detached((&nonce).into(), b"", &mut buffer)
                                    .map_err(|_| "encryption failed")?;
                                black_box((&buffer, tag));
                            }
                            Operation::Decrypt => {
                                buffer.extend_from_slice(black_box(&fixture));
                                cipher
                                    .decrypt_in_place_detached(
                                        (&fixture_nonce).into(),
                                        b"",
                                        &mut buffer,
                                        &tag,
                                    )
                                    .map_err(|_| "authentication failed")?;
                                black_box(&buffer);
                            }
                        }
                        Ok(())
                    };
                    let warmed = (|| -> Result<(), String> {
                        for _ in 0..c.warmup {
                            perform()?;
                        }
                        Ok(())
                    })();
                    ready.send(warmed.clone()).map_err(|e| e.to_string())?;
                    warmed?;
                    let start = rx.recv().map_err(|e| e.to_string())?;
                    let mut samples = Vec::new();
                    let mut count = 0u64;
                    let cycles_start = crate::cycles::current_thread();
                    while start.elapsed() < Duration::from_secs(u64::from(c.duration_seconds)) {
                        let batch_start = Instant::now();
                        for _ in 0..c.batch_size {
                            perform()?;
                        }
                        samples.push(
                            batch_start.elapsed().as_secs_f64() * 1e9 / f64::from(c.batch_size),
                        );
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
            ready_rx
                .recv()?
                .map_err(|e| format!("cipher preparation failed: {e}"))?;
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
            let (mut v, n, t, cycles) = handle.join().map_err(|_| "cipher worker failed")??;
            samples.append(&mut v);
            count += n;
            total_cycles =
                total_cycles.and_then(|total| cycles.and_then(|value| total.checked_add(value)));
            elapsed = elapsed.max(t);
        }
        if samples.is_empty() {
            return Err("No cipher operations completed".into());
        }
        Ok((samples, count, elapsed, total_cycles))
    })
}

pub fn run<C>(
    mut c: Config,
    input_config: InputConfig,
    name: &str,
    algorithm: &str,
) -> BenchResult<()>
where
    C: AeadInPlace + KeyInit + AeadCore<NonceSize = U12, TagSize = U16> + Send,
{
    if c.workers_values.is_empty()
        || c.workers_values.contains(&0)
        || c.duration_seconds == 0
        || c.repeats == 0
        || c.batch_size == 0
    {
        return Err("workers, duration, repeats, and batch_size must be positive".into());
    }
    let input = input_config.prepare()?;
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
    writeln!(log,"{name} | input_group=encryption input={} B mode={} key=32 B nonce=12 B tag=16 B aad=0 B | batch={} | buffer copy included",input.bytes.len(),input.mode,c.batch_size)?;
    let header = format!(
        "{:>3} {:>8} {:>7} {:>12} {:>8} {:>10} {:>10} {:>10} {:>10} {:>10} {:>14} {:>10}",
        "Tur",
        "Operation",
        "Workers",
        "Count",
        "Süre(sn)",
        "Ort.ns",
        "Medyan",
        "p95",
        "Min",
        "Maks",
        "ops/sn",
        "MiB/sn"
    );
    crate::console::start(
        name,
        true,
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
            for operation in [Operation::Encrypt, Operation::Decrypt] {
                let loading = crate::console::Loading::start();
                let result = benchmark::<C>(workers, &c, &input.bytes, operation);
                drop(loading);
                let (mut samples, count, elapsed, cycles) = result?;
                let cycles_per_byte = crate::cycles::per_byte(cycles, count, input.bytes.len());
                samples.sort_by(f64::total_cmp);
                let n = samples.len();
                let mean = samples.iter().sum::<f64>() / n as f64;
                let median = (samples[(n - 1) / 2] + samples[n / 2]) / 2.0;
                let p95 = samples[(n as f64 * 0.95).ceil() as usize - 1];
                let rate = count as f64 / elapsed;
                let line=format!("{:>3} {:>8} {:>7} {:>12} {:>8.3} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>14.2} {:>10.2}",round,operation.name(),workers,count,elapsed,mean,median,p95,samples[0],samples[n-1],rate,rate*input.bytes.len() as f64/1048576.0);
                crate::results::save_json(
                    algorithm,
                    serde_json::json!({"status":"success","round":round,"operation":operation.name(),"input_group":"encryption","input_mode":input.mode,"input_bytes":input.bytes.len(),"key_bytes":32,"nonce_bytes":12,"tag_bytes":16,"aad_bytes":0,"parameters":{"workers":workers,"warmup_per_worker":c.warmup,"duration_seconds":c.duration_seconds,"repeats":c.repeats,"batch_size":c.batch_size},"buffer_copy_included":true,"operation_count":count,"elapsed_seconds":elapsed,"cycles_per_byte":cycles_per_byte,"thread_cycles":cycles,"cycle_counter":"windows_query_thread_cycle_time","operations_per_second":rate,"mib_per_second":rate*input.bytes.len() as f64/1048576.0,"timing":{"unit":"ns","measurement":"batch_average_per_operation","batch_count":n,"mean":mean,"median":median,"p95":p95,"min":samples[0],"max":samples[n-1]}}),
                )?;
                console_rows.push(crate::console::Row {
                    algorithm: algorithm.to_owned(),
                    has_cycles: true,
                    cycles_per_byte,
                    argon: None,
                    scrypt: None,
                    group: String::new(),
                    workers,
                    round,
                    operation: Some(match operation {
                        Operation::Encrypt => "enc",
                        Operation::Decrypt => "dec",
                    }),
                    avg_ns: mean,
                    median_ns: median,
                    p95_ns: p95,
                    min_ns: samples[0],
                    max_ns: samples[n - 1],
                    rate,
                    mib: Some(rate * input.bytes.len() as f64 / 1048576.0),
                });
                crate::console::row(&console_rows, true);
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
    }
    crate::console::show(name, &console_rows, true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn verify<C>()
    where
        C: AeadInPlace + KeyInit + AeadCore<NonceSize = U12, TagSize = U16>,
    {
        let cipher = C::new_from_slice(&[0u8; 32]).unwrap();
        for size in [16, 1048576] {
            let plaintext = vec![42u8; size];
            let mut buffer = plaintext.clone();
            let nonce = [0u8; 12];
            let tag = cipher
                .encrypt_in_place_detached((&nonce).into(), b"", &mut buffer)
                .unwrap();
            let mut bad_tag = tag;
            bad_tag[0] ^= 1;
            let mut altered = buffer.clone();
            assert!(cipher
                .decrypt_in_place_detached((&nonce).into(), b"", &mut altered, &bad_tag)
                .is_err());
            cipher
                .decrypt_in_place_detached((&nonce).into(), b"", &mut buffer, &tag)
                .unwrap();
            assert_eq!(buffer, plaintext);
        }
    }
    #[test]
    fn authenticated_roundtrips_and_tampered_tags() {
        verify::<aes_gcm::Aes256Gcm>();
        verify::<chacha20poly1305::ChaCha20Poly1305>();
    }
    #[test]
    fn chacha20poly1305_rfc8439_vector() {
        let key: Vec<u8> = (0x80..=0x9f).collect();
        let nonce = [
            0x07, 0, 0, 0, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47,
        ];
        let aad = [
            0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7,
        ];
        let mut data = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.".to_vec();
        let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&key).unwrap();
        let tag = cipher
            .encrypt_in_place_detached((&nonce).into(), &aad, &mut data)
            .unwrap();
        let encoded: String = data.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            encoded,
            concat!(
                "d31a8d34648e60db7b86afbc53ef7ec2",
                "a4aded51296e08fea9e2b5a736ee62d6",
                "3dbea45e8ca9671282fafb69da92728b",
                "1a71de0a9e060b2905d6a5b67ecd3b36",
                "92ddbd7f2d778b8c9803aee328091b58",
                "fab324e4fad675945585808b4831d7bc",
                "3ff4def08e4b7a9de576d26586cec64b",
                "6116"
            )
        );
        assert_eq!(
            tag.as_slice(),
            &[
                0x1a, 0xe1, 0x0b, 0x59, 0x4f, 0x09, 0xe2, 0x6a, 0x7e, 0x90, 0x2e, 0xcb, 0xd0, 0x60,
                0x06, 0x91
            ]
        );
    }
    #[test]
    fn aes256_gcm_known_vector() {
        let cipher = aes_gcm::Aes256Gcm::new_from_slice(&[0u8; 32]).unwrap();
        let nonce = [0u8; 12];
        let mut data = [0u8; 16];
        let tag = cipher
            .encrypt_in_place_detached((&nonce).into(), b"", &mut data)
            .unwrap();
        assert_eq!(
            data,
            [
                0xce, 0xa7, 0x40, 0x3d, 0x4d, 0x60, 0x6b, 0x6e, 0x07, 0x4e, 0xc5, 0xd3, 0xba, 0xf3,
                0x9d, 0x18
            ]
        );
        assert_eq!(
            tag.as_slice(),
            &[
                0xd0, 0xd1, 0xc8, 0xa7, 0x99, 0x99, 0x6b, 0xf0, 0x26, 0x5b, 0x98, 0xb5, 0xd4, 0x8a,
                0xb9, 0x19
            ]
        );
    }
}
