use std::cell::RefCell;
use std::collections::HashSet;

#[derive(Default)]
struct ColumnState {
    last: Option<String>,
    index: usize,
}
#[derive(Default)]
struct ColumnOrders {
    w: ColumnState,
    m: ColumnState,
    t: ColumnState,
    p: ColumnState,
    log2n: ColumnState,
    r: ColumnState,
}
#[derive(Default)]
struct DisplayState {
    size: Option<(u16, u16)>,
    visible_lines: usize,
}
#[derive(Clone, Copy)]
struct MemoryColumns {
    kib: bool,
    mib: bool,
    kib_width: usize,
    mib_width: usize,
}
impl Default for MemoryColumns {
    fn default() -> Self {
        Self {
            kib: false,
            mib: true,
            kib_width: 6,
            mib_width: 6,
        }
    }
}
thread_local! {
    static COLUMN_ORDERS: RefCell<ColumnOrders> = RefCell::new(ColumnOrders::default());
    static DISPLAY_STATE: RefCell<DisplayState> = RefCell::new(DisplayState::default());
    static PRESET_NAME: RefCell<Option<String>> = const { RefCell::new(None) };
    static MEMORY_COLUMNS: RefCell<MemoryColumns> = RefCell::new(MemoryColumns::default());
}
use std::fmt;
use std::io::IsTerminal;

fn paint(text: impl fmt::Display, color: &str, enabled: bool) -> String {
    if enabled {
        format!("\x1b[{color}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}
fn stdout_color(text: impl fmt::Display, color: &str) -> String {
    paint(text, color, std::io::stdout().is_terminal())
}
pub fn set_preset(name: Option<String>) {
    PRESET_NAME.with(|preset| *preset.borrow_mut() = name);
}
fn format_mib(mib: f64) -> String {
    if mib.fract() == 0.0 {
        format!("{mib:.0}")
    } else {
        format!("{mib:.2}")
    }
}
pub fn set_argon_memory_profiles(values: impl Iterator<Item = u32>) {
    let mut columns = MemoryColumns {
        kib: false,
        mib: false,
        kib_width: 6,
        mib_width: 6,
    };
    for kib in values {
        if kib < 8 * 1024 {
            columns.kib = true;
            columns.kib_width = columns.kib_width.max(kib.to_string().len());
        } else {
            columns.mib = true;
            columns.mib_width = columns
                .mib_width
                .max(format_mib(f64::from(kib) / 1024.0).len());
        }
    }
    MEMORY_COLUMNS.with(|stored| *stored.borrow_mut() = columns);
}
pub fn locations(files: &[String]) {
    println!();
    println!(
        "{}{}",
        stdout_color("Results noted at ", "38;2;255;199;26"),
        stdout_color("/results.txt", "38;2;225;110;255")
    );
    for file in files {
        println!(
            "{}{}{}",
            stdout_color("Check ", "38;2;255;199;26"),
            stdout_color(format!("/results/{file}"), "38;2;225;110;255"),
            stdout_color(" for json file", "38;2;255;199;26")
        );
    }
    println!();
}
pub fn error(message: &str) {
    eprintln!(
        "{}",
        paint(
            format!("Error: {message}"),
            "31",
            std::io::stderr().is_terminal()
        )
    );
}

#[derive(Clone)]
pub struct Row {
    pub algorithm: String,
    pub has_cycles: bool,
    pub cycles_per_byte: Option<f64>,
    pub group: String,
    pub argon: Option<(f64, u32, u32)>,
    pub scrypt: Option<(u8, u32, u32)>,
    pub workers: u32,
    pub round: u32,
    pub operation: Option<&'static str>,
    pub avg_ns: f64,
    pub median_ns: f64,
    pub p95_ns: f64,
    pub min_ns: f64,
    pub max_ns: f64,
    pub rate: f64,
    pub mib: Option<f64>,
}
fn duration(ns: f64) -> String {
    let ms = ns / 1_000_000.0;
    if ms > 9999.0 {
        return "+9999ms".to_owned();
    }
    if ms < 0.001 {
        return "0.001ms".to_owned();
    }
    for precision in (0..=3).rev() {
        let formatted = format!("{ms:.precision$}ms");
        if formatted.len() <= 7 {
            return formatted;
        }
    }
    "+9999ms".to_owned()
}

fn number(n: f64) -> String {
    let value = format!("{n:.2}");
    let (integer, fraction) = value.split_once('.').unwrap();
    let mut chars = String::new();
    for (i, ch) in integer.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            chars.push(',');
        }
        chars.push(ch);
    }
    format!("{}.{fraction}", chars.chars().rev().collect::<String>())
}
fn hash_rate(rate: f64, hashing: bool) -> String {
    number(rate.clamp(0.01, if hashing { 99999.99 } else { 9999.99 }))
}
fn throughput(rate: f64) -> String {
    number(rate.clamp(0.01, 999999.99))
}

fn header(cipher: bool, argon: bool, scrypt: bool, has_mib: bool) -> Vec<String> {
    let mut h = vec!["#", "w"];
    if argon {
        MEMORY_COLUMNS.with(|columns| {
            let columns = columns.borrow();
            if columns.kib {
                h.push("m(KiB)");
            }
            if columns.mib {
                h.push("m(MiB)");
            }
        });
        h.extend(["t", "p"]);
    }
    if scrypt {
        h.extend(["log2n", "r", "p"]);
    }
    if cipher {
        h.push("op");
    }
    h.extend([
        "min",
        "avg",
        "med",
        "p95",
        "max",
        if cipher { "ops/s" } else { "hash/s" },
    ]);
    if has_mib {
        h.push("MiB/s");
        h.push("cyc/b");
    }
    h.into_iter().map(str::to_owned).collect()
}
fn cells(r: &Row, cipher: bool, argon: bool, scrypt: bool, summary: bool) -> Vec<String> {
    let dec = cipher && r.operation == Some("dec") && !summary;
    let mut c = vec![if summary || dec {
        String::new()
    } else {
        r.round.to_string()
    }];
    c.push(if dec {
        String::new()
    } else {
        r.workers.to_string()
    });
    if argon {
        MEMORY_COLUMNS.with(|columns| {
            let columns = columns.borrow();
            let memory = r.argon.map(|(m, _, _)| m);
            if columns.kib {
                c.push(
                    memory
                        .filter(|m| *m < 8.0)
                        .map(|m| format!("{:.0}", m * 1024.0))
                        .unwrap_or_else(|| "-".into()),
                );
            }
            if columns.mib {
                c.push(
                    memory
                        .filter(|m| *m >= 8.0)
                        .map(format_mib)
                        .unwrap_or_else(|| "-".into()),
                );
            }
        });
        if let Some((_, t, p)) = r.argon {
            c.extend([t.to_string(), p.to_string()]);
        } else {
            c.extend([String::new(), String::new()]);
        }
    }
    if scrypt {
        if let Some((log_n, r, p)) = r.scrypt {
            c.extend([log_n.to_string(), r.to_string(), p.to_string()]);
        } else {
            c.extend([String::new(), String::new(), String::new()]);
        }
    }
    if cipher {
        c.push(r.operation.unwrap_or("").into());
    }
    c.extend([
        duration(r.min_ns),
        duration(r.avg_ns),
        duration(r.median_ns),
        duration(r.p95_ns),
        duration(r.max_ns),
        if cipher {
            throughput(r.rate)
        } else {
            hash_rate(r.rate, r.mib.is_some())
        },
    ]);
    if let Some(mib) = r.mib {
        c.push(throughput(mib));
    }
    if r.has_cycles {
        c.push(
            r.cycles_per_byte
                .map(|value| format!("{value:.2}"))
                .unwrap_or_else(|| "-".into()),
        );
    }
    c
}
fn line(cells: &[String], widths: &[usize]) -> String {
    cells
        .iter()
        .zip(widths)
        .map(|(v, w)| format!("{v:>w$}"))
        .collect::<Vec<_>>()
        .join("   ")
}
// Each statistic is interpolated independently using the unrounded values.
fn gradient(value: f64, min: f64, max: f64) -> (u8, u8, u8) {
    let t = if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (
        (255.0 * t).round() as u8,
        (191.0 - 21.0 * t).round() as u8,
        0,
    )
}
fn column_color(header: &str, index: usize) -> &'static str {
    let starts_white = matches!(header, "w" | "t" | "r");
    if (index % 2 == 0) == starts_white {
        "97"
    } else if header == "w" {
        "38;2;248;255;173"
    } else {
        "38;2;171;255;255"
    }
}
fn column_rank(header: &str, value: &str) -> usize {
    COLUMN_ORDERS.with(|orders| {
        let mut orders = orders.borrow_mut();
        let values = match header {
            "w" => &mut orders.w,
            "m" | "m(KiB)" | "m(MiB)" => &mut orders.m,
            "log2n" => &mut orders.log2n,
            "r" => &mut orders.r,
            "t" => &mut orders.t,
            "p" => &mut orders.p,
            _ => unreachable!(),
        };
        if values.last.as_deref() != Some(value) {
            if values.last.is_some() {
                values.index += 1;
            }
            values.last = Some(value.to_owned());
        }
        values.index
    })
}
fn hash_speed_color(value: f64, min: f64, max: f64) -> (u8, u8, u8) {
    let t = if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.5
    };
    (
        (255.0 * (1.0 - t)).round() as u8,
        (170.0 + 21.0 * t).round() as u8,
        0,
    )
}

// The same boundary controls blank lines, colours, and metric comparisons.
fn starts_group(rows: &[Row], index: usize) -> bool {
    index == 0 || rows[index - 1].workers != rows[index].workers
}
fn group_index(rows: &[Row], index: usize) -> usize {
    (1..=index).filter(|&i| starts_group(rows, i)).count()
}
#[derive(Clone, Copy)]
enum Metric {
    Rate,
    Mib,
    Cycles,
}
fn metric_bounds(rows: &[Row], index: usize, metric: Metric) -> Option<(f64, f64)> {
    let mut start = index;
    while start > 0 && !starts_group(rows, start) {
        start -= 1;
    }
    let mut end = index + 1;
    while end < rows.len() && !starts_group(rows, end) {
        end += 1;
    }
    rows[start..end]
        .iter()
        .filter_map(|row| match metric {
            Metric::Rate => Some(row.rate),
            Metric::Mib => row.mib,
            Metric::Cycles => row.cycles_per_byte,
        })
        .filter(|value| value.is_finite())
        .fold(None, |acc, value| {
            Some(match acc {
                None => (value, value),
                Some((lo, hi)) => (f64::min(lo, value), f64::max(hi, value)),
            })
        })
}
fn group_color(index: usize) -> &'static str {
    if index % 2 == 0 {
        "97"
    } else {
        "38;2;248;255;173"
    }
}
fn result_line(
    cells: &[String],
    headers: &[String],
    widths: &[usize],
    row: &Row,
    bounds: (f64, f64),
    mib_bounds: Option<(f64, f64)>,
    cycle_bounds: Option<(f64, f64)>,
    group_index: usize,
    dashed: bool,
    _summary: bool,
) -> String {
    let first = cells
        .iter()
        .position(|value| !value.is_empty())
        .unwrap_or(0);
    let counter_color = group_color(group_index);
    cells
        .iter()
        .zip(headers)
        .zip(widths)
        .enumerate()
        .map(|(index, ((value, header), width))| {
            let padding = width.saturating_sub(value.chars().count());
            let leading = if dashed && index > first {
                stdout_color("-".repeat(padding), "38;2;40;40;40")
            } else {
                " ".repeat(padding)
            };
            let padded = value.to_owned();
            let time = match header.as_str() {
                "min" => Some(row.min_ns),
                "avg" => Some(row.avg_ns),
                "med" => Some(row.median_ns),
                "p95" => Some(row.p95_ns),
                "max" => Some(row.max_ns),
                _ => None,
            };
            let colored = if let Some(time) = time {
                let (r, g, b) = gradient(time, row.min_ns, row.max_ns);
                stdout_color(padded, &format!("38;2;{r};{g};{b}"))
            } else if matches!(header.as_str(), "hash/s" | "ops/s" | "MiB/s") {
                let (value, (min, max)) = if header == "MiB/s" {
                    let value = row.mib.expect("MiB/s column requires a value");
                    (value, mib_bounds.unwrap_or((value, value)))
                } else {
                    (row.rate, bounds)
                };
                let (r, g, b) = hash_speed_color(value, min, max);
                stdout_color(padded, &format!("38;2;{r};{g};{b}"))
            } else if header == "cyc/b" && row.cycles_per_byte.is_some() {
                let value = row.cycles_per_byte.unwrap();
                let (min, max) = cycle_bounds.unwrap_or((value, value));
                let (r, g, b) = hash_speed_color(max - value, 0.0, max - min);
                stdout_color(padded, &format!("38;2;{r};{g};{b}"))
            } else if matches!(header.as_str(), "#" | "w") && !value.is_empty() {
                stdout_color(padded, counter_color)
            } else if matches!(
                header.as_str(),
                "m(KiB)" | "m(MiB)" | "t" | "p" | "log2n" | "r"
            ) && !value.is_empty()
                && value != "-"
            {
                stdout_color(padded, column_color(header, column_rank(header, value)))
            } else {
                stdout_color(padded, "97")
            };
            let separator = if index == 0 {
                String::new()
            } else if dashed && index > first {
                stdout_color("---", "38;2;40;40;40")
            } else {
                "   ".to_owned()
            };
            format!("{separator}{leading}{colored}")
        })
        .collect::<Vec<_>>()
        .concat()
}
pub fn start(
    name: &str,
    cipher: bool,
    argon: bool,
    scrypt: bool,
    has_mib: bool,
    timing: (u32, u32, u32, Option<u32>),
) {
    COLUMN_ORDERS.with(|orders| *orders.borrow_mut() = ColumnOrders::default());
    DISPLAY_STATE.with(|state| {
        *state.borrow_mut() = DisplayState {
            size: if std::io::stdout().is_terminal() {
                crossterm::terminal::size().ok()
            } else {
                None
            },
            visible_lines: 0,
        }
    });
    let h = header(cipher, argon, scrypt, has_mib);
    let widths = live_widths(&h);
    let rule = "─".repeat(widths.iter().sum::<usize>() + 3 * (widths.len() - 1));
    let title = PRESET_NAME.with(|preset| match preset.borrow().as_deref() {
        Some(preset) => format!("{name} - {preset}"),
        None => name.to_owned(),
    });
    let (duration, warmup, repeats, batch_size) = timing;
    let mut title = format!("{title} - d{duration}/w{warmup}/r{repeats}");
    if let Some(batch_size) = batch_size {
        title.push_str(&format!("/b{batch_size}"));
    }
    println!(
        "\n{}\n{}\n{}",
        stdout_color(title, "97"),
        stdout_color(rule, "90"),
        stdout_color(line(&h, &widths), "97")
    );
}
fn live_widths(h: &[String]) -> Vec<usize> {
    h.iter()
        .map(|label| match label.as_str() {
            "avg" | "med" | "p95" | "min" | "max" => 7,
            "hash/s" if h.iter().any(|label| label == "MiB/s") => 9,
            "hash/s" => 8,
            "ops/s" | "MiB/s" | "cyc/b" => 10,
            "m(KiB)" => MEMORY_COLUMNS.with(|columns| columns.borrow().kib_width),
            "m(MiB)" => MEMORY_COLUMNS.with(|columns| columns.borrow().mib_width),
            "log2n" => 5,
            "#" | "t" | "p" | "w" | "r" => 3,
            _ => label.chars().count().max(3),
        })
        .collect()
}
// Repaint complete visible rows, accounting for terminal line wrapping.
pub fn row(rows: &[Row], cipher: bool) {
    use std::io::Write;
    let Some(latest) = rows.last() else {
        return;
    };
    let argon = latest.argon.is_some();
    let scrypt = latest.scrypt.is_some();
    let h = header(cipher, argon, scrypt, latest.mib.is_some());
    let widths = live_widths(&h);
    let terminal = std::io::stdout().is_terminal();
    let size = if terminal {
        crossterm::terminal::size().ok()
    } else {
        None
    };
    // Rebuild colours from the beginning so alternating parameter colours remain stable.
    COLUMN_ORDERS.with(|orders| *orders.borrow_mut() = ColumnOrders::default());
    let mut rendered: Vec<(String, usize)> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 && starts_group(rows, index) {
            rendered.push((String::new(), 0));
        }
        if !row.group.is_empty() && !argon && !scrypt {
            rendered.push((stdout_color(&row.group, "97"), row.group.chars().count()));
        }
        let mut c = cells(row, cipher, argon, scrypt, false);
        if cipher && row.operation == Some("dec") {
            c[0] = row.round.to_string();
            c[1] = row.workers.to_string();
        }
        let length = line(&c, &widths).chars().count();
        let bounds = metric_bounds(rows, index, Metric::Rate).unwrap();
        let bounds_mib = metric_bounds(rows, index, Metric::Mib);
        let bounds_cycles = metric_bounds(rows, index, Metric::Cycles);
        rendered.push((
            result_line(
                &c,
                &h,
                &widths,
                row,
                bounds,
                bounds_mib,
                bounds_cycles,
                group_index(rows, index),
                true,
                false,
            ),
            length,
        ));
    }
    let worker_changed = rows.len() > 1 && starts_group(rows, rows.len() - 1);
    let new_lines = 1
        + usize::from(!latest.group.is_empty() && !argon && !scrypt)
        + usize::from(worker_changed);
    let previous = rendered.len() - new_lines;
    let (start_index, rewind, can_clear) = DISPLAY_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let same_size = state.size == size;
        let Some((columns, height)) = size.filter(|(w, h)| terminal && *w > 0 && *h > 2) else {
            state.size = size;
            state.visible_lines = 0;
            return (previous, 0, false);
        };
        let columns = columns as usize;
        let limit = height.saturating_sub(2) as usize;
        let physical = |length: usize| length.max(1).div_ceil(columns);
        let new_height: usize = rendered[previous..]
            .iter()
            .map(|(_, len)| physical(*len))
            .sum();
        let allowance = if same_size {
            state.visible_lines.min(limit)
        } else {
            0
        };
        let mut rewind = 0;
        let mut start_index = previous;
        while start_index > 0 {
            let n = physical(rendered[start_index - 1].1);
            if rewind + n > allowance {
                break;
            }
            rewind += n;
            start_index -= 1;
        }
        state.size = size;
        // Track only complete logical lines that remain wholly visible for the next update.
        let mut visible = 0;
        for (_, length) in rendered[start_index..].iter().rev() {
            let n = physical(*length);
            if visible + n > limit {
                break;
            }
            visible += n;
        }
        state.visible_lines = if new_height <= limit { visible } else { 0 };
        (start_index, rewind, true)
    });
    if rewind > 0 {
        print!("\x1b[{rewind}A\r");
    }
    for (text, _) in &rendered[start_index..] {
        if can_clear {
            print!("\r\x1b[2K");
        }
        println!("{text}");
    }
    let _ = std::io::stdout().flush();
}

pub struct Loading {
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Loading {
    pub fn start() -> Self {
        use std::io::IsTerminal;
        if !std::io::stdout().is_terminal() {
            return Self {
                stop: None,
                thread: None,
            };
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            use std::io::Write;
            let frames = ['|', '/', '-', '\\'];
            let mut i = 0;
            loop {
                print!(
                    "\r{}",
                    stdout_color(format!("{} Running...", frames[i % frames.len()]), "33")
                );
                let _ = std::io::stdout().flush();
                i += 1;
                if rx
                    .recv_timeout(std::time::Duration::from_millis(120))
                    .is_ok()
                {
                    break;
                }
            }
            print!("\r\x1b[2K");
            let _ = std::io::stdout().flush();
        });
        Self {
            stop: Some(tx),
            thread: Some(thread),
        }
    }
}
impl Drop for Loading {
    fn drop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn show(_name: &str, rows: &[Row], cipher: bool) {
    let argon = rows.iter().any(|r| r.argon.is_some());
    let scrypt = rows.iter().any(|r| r.scrypt.is_some());
    let h = header(
        cipher,
        argon,
        scrypt,
        rows.iter().any(|row| row.mib.is_some()),
    );
    let mut groups: Vec<(&str, u32)> = Vec::new();
    for r in rows {
        if !groups.contains(&(r.group.as_str(), r.workers)) {
            groups.push((&r.group, r.workers));
        }
    }
    let mut summaries: Vec<(Vec<String>, Option<Row>)> = Vec::new();
    for (group, workers) in &groups {
        if !group.is_empty() && !argon && !scrypt {
            summaries.push((vec![group.to_string()], None));
        }
        let mut seen = HashSet::new();
        for r in rows
            .iter()
            .filter(|r| r.group == *group && r.workers == *workers)
        {
            if !seen.insert(r.operation) {
                continue;
            }
            let v: Vec<&Row> = rows
                .iter()
                .filter(|x| {
                    x.group == *group && x.workers == *workers && x.operation == r.operation
                })
                .collect();
            let n = v.len() as f64;
            let avg = Row {
                group: r.group.clone(),
                algorithm: r.algorithm.clone(),
                has_cycles: r.has_cycles,
                cycles_per_byte: v
                    .iter()
                    .try_fold(0.0, |sum, x| Some(sum + x.cycles_per_byte?))
                    .map(|sum| sum / n),
                argon: r.argon,
                scrypt: r.scrypt,
                workers: *workers,
                round: 0,
                operation: r.operation,
                avg_ns: v.iter().map(|x| x.avg_ns).sum::<f64>() / n,
                median_ns: v.iter().map(|x| x.median_ns).sum::<f64>() / n,
                p95_ns: v.iter().map(|x| x.p95_ns).sum::<f64>() / n,
                min_ns: v.iter().map(|x| x.min_ns).sum::<f64>() / n,
                max_ns: v.iter().map(|x| x.max_ns).sum::<f64>() / n,
                rate: v.iter().map(|x| x.rate).sum::<f64>() / n,
                mib: r
                    .mib
                    .map(|_| v.iter().map(|x| x.mib.unwrap()).sum::<f64>() / n),
            };
            summaries.push((cells(&avg, cipher, argon, scrypt, true), Some(avg)));
        }
    }
    let widths = live_widths(&h);
    let width = widths.iter().sum::<usize>() + 3 * (widths.len() - 1);
    let rule = "─".repeat(width);
    let mut summary_header = h.clone();
    summary_header[0].clear();
    println!(
        "\n{}\n{}\n{}",
        stdout_color("Summary (mean of runs)", "97"),
        stdout_color(rule, "90"),
        stdout_color(line(&summary_header, &widths), "97")
    );
    // Start the summary's colour alternation from its own first row.
    COLUMN_ORDERS.with(|orders| *orders.borrow_mut() = ColumnOrders::default());
    let summary_rows: Vec<Row> = summaries
        .iter()
        .filter_map(|(_, row)| row.clone())
        .collect();
    let mut index = 0;
    for (cells, row) in &summaries {
        match row {
            None if cells.is_empty() => println!(),
            None => println!("{}", stdout_color(&cells[0], "97")),
            Some(row) => {
                let bounds = metric_bounds(&summary_rows, index, Metric::Rate).unwrap();
                let bounds_mib = metric_bounds(&summary_rows, index, Metric::Mib);
                let bounds_cycles = metric_bounds(&summary_rows, index, Metric::Cycles);
                println!(
                    "{}",
                    result_line(
                        cells,
                        &h,
                        &widths,
                        row,
                        bounds,
                        bounds_mib,
                        bounds_cycles,
                        group_index(&summary_rows, index),
                        true,
                        true
                    )
                );
                index += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argon_memory_columns_follow_threshold_and_keep_profiles_aligned() {
        assert_eq!(format_mib(8.0), "8");
        assert_eq!(format_mib(19.5), "19.50");
        assert_eq!(format_mib(8193.0 / 1024.0), "8.00");
        assert_eq!(format_mib(20000.0 / 1024.0), "19.53");

        set_argon_memory_profiles([32, 8191, 8192, 20000].into_iter());
        let headings = header(false, true, false, false);
        assert_eq!(&headings[2..6], &["m(KiB)", "m(MiB)", "t", "p"]);
        let mut small = row(1.0, None);
        small.argon = Some((8191.0 / 1024.0, 1, 1));
        let mut large = row(1.0, None);
        large.argon = Some((20000.0 / 1024.0, 1, 1));
        let small_cells = cells(&small, false, true, false, false);
        let large_cells = cells(&large, false, true, false, false);
        assert_eq!(&small_cells[2..4], &["8191", "-"]);
        assert_eq!(&large_cells[2..4], &["-", "19.53"]);
        assert_eq!(
            line(&small_cells, &live_widths(&headings)).len(),
            line(&large_cells, &live_widths(&headings)).len()
        );

        set_argon_memory_profiles([32].into_iter());
        assert_eq!(header(false, true, false, false)[2], "m(KiB)");
        set_argon_memory_profiles([8192].into_iter());
        assert_eq!(header(false, true, false, false)[2], "m(MiB)");
        MEMORY_COLUMNS.with(|columns| *columns.borrow_mut() = MemoryColumns::default());
    }
    fn row(rate: f64, operation: Option<&'static str>) -> Row {
        Row {
            algorithm: "test".into(),
            has_cycles: false,
            cycles_per_byte: None,
            group: String::new(),
            argon: None,
            scrypt: None,
            workers: 1,
            round: 1,
            operation,
            avg_ns: 1.0,
            median_ns: 1.0,
            p95_ns: 1.0,
            min_ns: 1.0,
            max_ns: 1.0,
            rate,
            mib: None,
        }
    }
    #[test]
    fn metrics_follow_blank_line_groups() {
        let mut a = row(10.0, Some("enc"));
        a.mib = Some(100.0);
        a.cycles_per_byte = Some(4.0);
        a.argon = Some((19.0, 1, 1));
        let mut b = row(20.0, Some("dec"));
        b.mib = Some(200.0);
        b.cycles_per_byte = Some(2.0);
        b.argon = Some((64.0, 1, 1));
        let mut c = row(100.0, Some("enc"));
        c.workers = 2;
        c.mib = Some(500.0);
        c.cycles_per_byte = None;
        let mut d = row(50.0, None);
        d.workers = 1;
        d.cycles_per_byte = Some(9.0);
        let rows = [a, b, c, d];
        assert_eq!(metric_bounds(&rows, 0, Metric::Rate), Some((10.0, 20.0)));
        assert_eq!(metric_bounds(&rows, 1, Metric::Mib), Some((100.0, 200.0)));
        assert_eq!(metric_bounds(&rows, 0, Metric::Cycles), Some((2.0, 4.0)));
        assert_eq!(metric_bounds(&rows, 2, Metric::Cycles), None);
        assert_eq!(metric_bounds(&rows, 3, Metric::Rate), Some((50.0, 50.0)));
        assert_eq!(
            (
                group_index(&rows, 0),
                group_index(&rows, 1),
                group_index(&rows, 2),
                group_index(&rows, 3)
            ),
            (0, 0, 1, 2)
        );
        assert_eq!(group_color(0), "97");
        assert_eq!(group_color(1), "38;2;248;255;173");
        assert_eq!(hash_speed_color(4.0 - 2.0, 0.0, 2.0), (0, 191, 0));
        assert_eq!(hash_speed_color(4.0 - 4.0, 0.0, 2.0), (255, 170, 0));
        assert_eq!(hash_speed_color(0.0, 0.0, 0.0), (128, 181, 0));
    }
    #[test]
    fn hash_rate_format_boundaries() {
        for (input, expected) in [
            (0.0, "0.01"),
            (0.01, "0.01"),
            (99.99, "99.99"),
            (100.0, "100.00"),
            (999.99, "999.99"),
            (1000.1, "1,000.10"),
            (9999.99, "9,999.99"),
            (10000.0, "9,999.99"),
        ] {
            assert_eq!(hash_rate(input, false), expected);
        }
        assert_eq!(hash_rate(10000.0, true), "10,000.00");
        assert_eq!(hash_rate(99999.99, true), "99,999.99");
        assert_eq!(hash_rate(100000.0, true), "99,999.99");
        for (input, expected) in [
            (0.0, "0.01"),
            (0.01, "0.01"),
            (1234.56, "1,234.56"),
            (999999.99, "999,999.99"),
            (1000000.0, "999,999.99"),
        ] {
            assert_eq!(throughput(input), expected);
        }
        assert_eq!(
            live_widths(&header(false, true, false, false)).last(),
            Some(&8)
        );
        assert_eq!(live_widths(&header(false, false, false, true))[7], 9);
        assert_eq!(
            live_widths(&header(true, false, false, true)).last(),
            Some(&10)
        );
    }
    #[test]
    fn millisecond_column_limits_and_rounding() {
        for (ms, expected) in [
            (0.0, "0.001ms"),
            (0.001, "0.001ms"),
            (1.123, "1.123ms"),
            (12.12, "12.12ms"),
            (123.1, "123.1ms"),
            (1234.0, "1234ms"),
            (9999.0, "9999ms"),
            (10000.0, "+9999ms"),
            (9.9999, "10.00ms"),
            (99.9999, "100.0ms"),
        ] {
            assert_eq!(duration(ms * 1_000_000.0), expected);
        }
    }
    #[test]
    fn gradient_and_worker_order() {
        assert_eq!(gradient(10.0, 10.0, 20.0), (0, 191, 0));
        assert_eq!(gradient(20.0, 10.0, 20.0), (255, 170, 0));
        assert_eq!(gradient(15.0, 10.0, 20.0), (128, 181, 0));
        assert_eq!(gradient(10.0, 10.0, 10.0), (0, 191, 0));
        assert_eq!(column_color("w", 0), "97");
        assert_eq!(column_color("w", 1), "38;2;248;255;173");
        assert_eq!(column_color("w", 2), "97");
        assert_eq!(column_color("t", 0), "97");
        assert_eq!(column_color("t", 1), "38;2;171;255;255");
        assert_eq!(column_color("t", 2), "97");
        for header in ["m", "p"] {
            assert_eq!(column_color(header, 0), "38;2;171;255;255");
            assert_eq!(column_color(header, 1), "97");
            assert_eq!(column_color(header, 2), "38;2;171;255;255");
        }
        COLUMN_ORDERS.with(|orders| *orders.borrow_mut() = ColumnOrders::default());
        assert_eq!(column_rank("w", "1"), 0);
        assert_eq!(column_rank("w", "2"), 1);
        assert_eq!(column_rank("w", "1"), 2);
        assert_eq!(column_rank("m", "19.00"), 0);
        assert_eq!(column_rank("m", "19.00"), 0);
        assert_eq!(column_rank("m", "32.00"), 1);
        assert_eq!(column_rank("m", "64.00"), 2);
        assert_eq!(column_rank("m", "19.00"), 3);
        assert_eq!(column_rank("p", "1"), 0);
        assert_eq!(column_rank("t", "1"), 0);
    }
}
