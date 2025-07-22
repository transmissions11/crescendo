use std::io;

use core_affinity::CoreId;
use rlimit::Resource;

use crate::config;

/// Increase the file descriptor limit to the given minimum.
///
/// Panics if the hard limit is too low, otherwise tries to increase.
pub fn increase_nofile_limit(min_limit: u64) -> io::Result<u64> {
    let (soft, hard) = Resource::NOFILE.get()?;
    println!("[*] At startup, file descriptor limit:      soft = {soft}, hard = {hard}");

    if hard < min_limit {
        panic!("[!] File descriptor hard limit is too low. Please increase it to at least {}.", min_limit);
    }

    if soft != hard {
        Resource::NOFILE.set(hard, hard)?; // Just max things out to give us plenty of overhead.
        let (soft, hard) = Resource::NOFILE.get()?;
        println!("[+] After increasing file descriptor limit: soft = {soft}, hard = {hard}");
    }

    Ok(soft)
}

/// Pin the current thread to the given core ID if enabled.
///
/// Panics if the thread fails to pin.
pub fn maybe_pin_thread(core_id: CoreId) {
    if !config::get().workers.thread_pinning {
        return;
    }

    if !core_affinity::set_for_current(core_id) {
        panic!("[!] Failed to pin thread to core {}.", core_id.id);
    }
}

/// Format a sorted list of numbers into a range string (e.g., "1-3, 5, 7-9")
pub fn format_ranges(nums: &[usize]) -> String {
    if nums.is_empty() {
        return String::new();
    }

    let mut ranges = Vec::new();
    let mut i = 0;

    while i < nums.len() {
        let start = nums[i];
        let mut end = start;

        // Find end of consecutive sequence
        while i + 1 < nums.len() && nums[i + 1] == nums[i] + 1 {
            i += 1;
            end = nums[i];
        }

        // Format this range
        if start == end {
            ranges.push(start.to_string());
        } else {
            ranges.push(format!("{}-{}", start, end));
        }

        i += 1;
    }

    ranges.join(", ")
}
