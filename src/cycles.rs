/// Thread cycles during the timed benchmark loop; unavailable counters remain None.
#[cfg(windows)]
pub fn current_thread() -> Option<u64> {
    use windows_sys::Win32::System::{
        Threading::GetCurrentThread, WindowsProgramming::QueryThreadCycleTime,
    };
    let mut cycles = 0;
    // GetCurrentThread returns a valid pseudo-handle for the calling thread.
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    if ok != 0 {
        Some(cycles)
    } else {
        None
    }
}
#[cfg(not(windows))]
pub fn current_thread() -> Option<u64> {
    None
}

pub fn delta(start: Option<u64>, end: Option<u64>) -> Option<u64> {
    end?.checked_sub(start?)
}
pub fn per_byte(cycles: Option<u64>, count: u64, bytes: usize) -> Option<f64> {
    if count == 0 || bytes == 0 {
        return None;
    }
    Some(cycles? as f64 / (count as f64 * bytes as f64))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_missing_and_zero_data() {
        assert_eq!(delta(Some(100), Some(250)), Some(150));
        assert_eq!(delta(Some(250), Some(100)), None);
        assert_eq!(delta(None, Some(100)), None);
        assert_eq!(per_byte(Some(600), 3, 20), Some(10.0));
        assert_eq!(per_byte(Some(600), 3, 0), None);
        assert_eq!(per_byte(None, 3, 20), None);
    }
}
