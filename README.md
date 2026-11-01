# CryptBench

A benchmark tool written in Rust for measuring hashing performance and more.

| Category | Supported Algorithms |
| --- | --- |
| Password hashing | Argon2id, scrypt |
| Hashing | SHA-2 & SHA-3 (256, 384, 512), BLAKE3 |
| Cipher | AES-256-GCM, ChaCha20-Poly1305 |


## Quick start

Edit [config.json](config.json) and a preset in `presets`, then run `start.cmd`. Select an algorithm in<br> cli and press Enter. You can use change preset button below the algorithms to choose<br> another YAML file from the `presets` directory.

The launcher prepares the C++ build tools and SDK in the background while the menu is<br> open, then runs the benchmark in release mode. Run one benchmark instance at a time.

### **Requirements**

- Rust and Cargo with the Windows MSVC toolchain.
- Visual Studio C++ build tools and a Windows SDK.
- Windows PowerShell for the interactive launcher.

## Command line

```powershell
cargo run --release -- --algorithm argon2id --config config.json
cargo run --release -- --algorithm sha256 --config config.json
cargo run --release -- --algorithm aes256gcm --config config.json
```

Available options:

| Option | Description | Default |
| --- | --- | --- |
| `--algorithm <ALGORITHM>` | Select `argon2id`, `sha2`, `sha3`, `sha256`, `sha384`, `sha512`, `sha3-256`, `sha3-384`, `sha3-512`, `blake3`, `scrypt`, `aes256gcm`, or `chacha20poly1305`. | `argon2id` |
| `--config <PATH>` | Read input groups and the default preset from a JSON configuration file. | No file |
| `--preset <PATH>` | Override the preset from the configuration for this run; relative to the current directory. Requires `--config`. | Preset in configuration |
| `--m <KIB>` | Argon2id memory per worker in KiB. | `19456` |
| `--t <COUNT>` | Argon2id iteration count. | `2` |
| `--p <COUNT>` | Argon2id lane count. | `1` |
| `--profiles` | Run the built-in Argon2id memory and iteration profiles. | Off |
| `--warmup <COUNT>` | Warmup operations per worker. | `10` |
| `--duration <SECONDS>` | Measurement time per run in seconds. | `5` |
| `--workers <COUNT>` | Number of independent workers. | `1` |
| `--repeats <COUNT>` | Number of rounds. | `1` |
| `-h`, `--help` | Show command-line help. | — |

`--config` cannot be combined with individual benchmark options. `--m`, `--t`, and `--p` configure Argon2id; they cannot be combined with `--profiles`. The command-line `--m` is in KiB, while `m_mb` in the configuration is in MiB. Without `--config`, input defaults to `benchmark-input!` (16 bytes); SHA and BLAKE3 batch size is `1024`, cipher batch size is `16`, and scrypt uses `log_n=14`, `r=8`, `p=1`. Use `config.json` to change input groups and the default preset; use the preset to change SHA variants, batch sizes, or scrypt parameters.

## Configuration

`config.json` contains the input groups, `workers_values` shared by all algorithms, and<br> a `preset` path relative to the configuration file. The preset contains algorithm settings.<br> Copy an existing file in `presets` to create another preset. Selecting a preset in the launcher<br> saves it as the default in `config.json`. Unknown fields are rejected. SHA-2 and SHA-3<br> variants share their respective family settings.

Each group has a `value`. When `size_bytes` is omitted, the UTF-8 bytes of `value` are used<br> as written. When it is set to a positive integer, the bytes repeat or truncate to that exact length;<br> `value` must then be nonempty. For example, 1 MiB is 1,048,576 bytes. Truncation may cut<br> through a multibyte UTF-8 character.

### Shared settings

- `workers_values` in `config.json`: positive independent worker counts shared by<br> every algorithm; each is measured separately.
- `duration_seconds`: positive measurement duration per run, excluding warmup.
- `warmup`: unmeasured operations per worker before each run; may be zero.
- `repeats`: positive number of rounds; defaults to `1`. Worker counts run in ascending<br> order, with all rounds completed for each profile before moving to the next.
- `batch_size`: positive operations per timed SHA, BLAKE3, or encryption batch. The<br> default is `1024` for SHA/BLAKE3 and `16` for encryption.

Workers finish warmup before a shared measurement clock starts. Once the duration expires,<br> an in-progress operation or batch finishes, so actual elapsed time may be longer.

### Argon2id

`m_mb` sets memory per worker in MiB (1 MiB = 1024 KiB).<br>
`t` sets iterations, and `p_values` selects Argon2 lane counts.<br>
All parameter and worker combinations are measured.

The console shows memory below 8 MiB in KiB and memory of 8 MiB or more in MiB.<br> Fractional MiB values are rounded to two decimal places. If both ranges are present,<br> the table displays a separate column for each unit.

Each worker reuses its own memory buffer. Approximate working memory is `m_mb × workers` MiB.<br> The current implementation processes Argon2 lanes sequentially; `p` does not set the worker count.<br> Individual latency excludes salt generation; overall throughput includes it.<br> Each hash uses a fresh 16-byte salt and produces 32 bytes.

### scrypt

`log_n_values`, `r_values`, and `p_values` select scrypt parameters, with `N = 2^log_n`.<br> All combinations with `workers_values` are measured.<br> The primary working array uses approximately `128 × N × r` bytes per worker. `p` is an algorithm<br> parameter, not the worker count.

Each hash uses a fresh 16-byte salt and produces 32 bytes. Working memory is allocated per hash. Individual<br> latency includes allocation and computation but excludes salt generation; overall throughput includes it.

### SHA and BLAKE3

`sha2` and `sha3` each have a `variants` list containing distinct values chosen from `256`, `384`, and `512`.<br> Selecting a family in the launcher runs the configured variants in order. Individual variants can also be selected<br> on the command line; they use the shared family settings regardless of list selection. BLAKE3 has its own settings<br> and a 32-byte output.

Hashes are measured in batches. Reported per-hash timings are batch averages; their median and p95 do not<br> describe individual hash latencies. BLAKE3 uses its single-threaded per-hash API, with independent concurrency<br> controlled by `workers_values`.

### Encryption

`aes256gcm` and `chacha20poly1305` each have their own settings. Encryption and authenticated decryption are<br> measured separately for each worker count and round. Both use 32-byte keys, 12-byte nonces, 16-byte tags,<br> and empty AAD. Keys are generated per worker; encryption uses a distinct counter nonce for each operation.<br> Decryption verifies a valid ciphertext and tag prepared before measurement.

Buffers are allocated before warmup and reused. Each timed operation includes copying the plaintext or ciphertext<br> into the buffer and authentication; encryption also includes nonce preparation. Key setup and fixture preparation<br> are excluded. Timing statistics are batch averages per operation.

## Results

### Console output

The console prints each completed measurement as a row and displays a summary after the test.<br> The heading identifies the algorithm, preset, duration, warmup, repeats, and batch size when<br> applicable. Rows are separated into groups by the worker count. Only visible rows can be repainted<br> as new measurements change the colour range. Rows outside the terminal viewport cannot be<br> repainted. The summary shows the arithmetic mean of each statistic across rounds. Its median and<br> p95 are averages of the individual round statistics, not percentiles calculated from all samples combined.<br> Encryption and decryption are summarized separately.

### Metrics

- `hash/s` and `ops/s`: completed hashes or operations across all workers, divided by elapsed time<br> from the common start until the last worker finishes.
- `MiB/s`: throughput multiplied by input bytes and divided by 1,048,576.
- `min`, `avg`, `med`, `p95`, and `max`: timing statistics; p95 uses the nearest-rank method. SHA, BLAKE3,<br> and encryption timings describe batch averages per operation.
- `cyc/b`: summed worker thread cycles divided by input bytes processed,<br> for SHA, BLAKE3, and encryption. On Windows, cycles come from `QueryThreadCycleTime`. This<br> includes worker-side measurement overhead and, for encryption, buffer copies and nonce preparation<br> where applicable. It is not a pure cryptographic instruction count.<br> The value is `-` for empty inputs or unavailable counters; CPU and VM counter implementations can differ.

Console durations are displayed in milliseconds with a maximum of seven characters, from `0.001ms` to `+9999ms`.<br> Console `hash/s` is limited to 0.01–9,999.99 for password hashing and 0.01–99,999.99 for SHA and BLAKE3;<br> `ops/s` and `MiB/s` are limited to 0.01–999,999.99. Values outside these ranges are displayed at the nearest limit.<br> JSON and TXT retain the measured values.

### Result files

- `results.txt` in the project root receives appended text output for completed measurements.
- `results/` receives one JSON file per successful test invocation, containing all rounds, profiles,<br> workers, and selected SHA variants. Files are named `result-of-(algorithm)-DD.MM.YYYY-HH.MM.SS.json`;<br> a numeric suffix prevents overwriting an existing file.

The JSON file includes a completion timestamp and a `measurements` array with per-measurement timestamps,<br> parameters, input metadata, operation counts, throughput, and timing statistics. It also stores `cycles_per_byte`<br> and `thread_cycles` where applicable. If a test fails, completed text output may remain in `results.txt`, but the<br> invocation does not produce a successful JSON result.

Results depend on CPU frequency, temperature, memory bandwidth, and background workloads. Different algorithms<br> perform different amounts of work, so their operation rates are not equivalent security costs.

## License

[MIT](LICENSE) © 2026 burakburaq.
