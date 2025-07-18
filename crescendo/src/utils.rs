use std::io;

use core_affinity::CoreId;
use rlimit::Resource;

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

pub fn maybe_pin_thread(core_id: CoreId, enable_thread_pinning: bool) {
    if !enable_thread_pinning {
        return;
    }

    if !core_affinity::set_for_current(core_id) {
        panic!("[!] Failed to pin thread to core {}.", core_id.id);
    }
}
