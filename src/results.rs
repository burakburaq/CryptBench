use crate::BenchResult;
use std::cell::RefCell;
thread_local! {
    static SAVED_FILES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static PENDING: RefCell<Vec<serde_json::Value>> = const { RefCell::new(Vec::new()) };
    static RUN_ALGORITHM: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn begin(algorithm: &str) {
    RUN_ALGORITHM.with(|name| *name.borrow_mut() = algorithm.replace('-', ""));
    PENDING.with(|rows| rows.borrow_mut().clear());
}

use std::{
    fs::OpenOptions,
    io::{ErrorKind, Write},
    path::PathBuf,
};

/// Buffer each completed measurement until the full test succeeds.
pub fn save_json(algorithm: &str, mut result: serde_json::Value) -> BenchResult<()> {
    let now = chrono::Local::now();
    result["algorithm"] = algorithm.into();
    result["timestamp"] = now.to_rfc3339().into();
    result["unix_timestamp_ms"] = now.timestamp_millis().into();
    PENDING.with(|rows| rows.borrow_mut().push(result));
    Ok(())
}

/// Write a single JSON file for the completed invocation, including all measurements.
pub fn finish() -> BenchResult<()> {
    let measurements = PENDING.with(|rows| std::mem::take(&mut *rows.borrow_mut()));
    if measurements.is_empty() {
        return Ok(());
    }
    let algorithm = RUN_ALGORITHM.with(|name| name.borrow().clone());
    let now = chrono::Local::now();
    let result = serde_json::json!({
        "algorithm": algorithm, "status": "success", "timestamp": now.to_rfc3339(),
        "unix_timestamp_ms": now.timestamp_millis(), "os": std::env::consts::OS,
        "architecture": std::env::consts::ARCH, "measurements": measurements
    });
    let bytes = serde_json::to_vec_pretty(&result)?;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    std::fs::create_dir_all(&dir)?;
    let base = format!(
        "result-of-{algorithm}-{}-{}",
        now.format("%d.%m.%Y"),
        now.format("%H.%M.%S")
    );
    let mut suffix = 0u64;
    loop {
        let filename = if suffix == 0 {
            format!("{base}.json")
        } else {
            format!("{base}-{suffix}.json")
        };
        let path = dir.join(&filename);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let written = file
                    .write_all(&bytes)
                    .and_then(|_| file.write_all(b"\n"))
                    .and_then(|_| file.flush());
                if let Err(error) = written {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    return Err(error.into());
                }
                SAVED_FILES.with(|files| files.borrow_mut().push(filename));
                return Ok(());
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => suffix += 1,
            Err(error) => return Err(error.into()),
        }
    }
}

/// Show only files actually written by this process, including partial runs.
pub fn print_saved() {
    SAVED_FILES.with(|files| {
        let files = files.borrow();
        if files.is_empty() {
            return;
        }
        crate::console::locations(&files);
    });
}
