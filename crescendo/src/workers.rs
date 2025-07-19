use std::collections::HashMap;

use core_affinity::CoreId;

mod network;
mod tx_gen;

pub use network::network_worker;
pub use tx_gen::tx_gen_worker;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum WorkerType {
    TxGen,
    Network,
}

/// Given a desired breakdown of workers, translate this into actual numbers of workers to spawn.
///
/// Assumes that thread pinning is desired and thus maps each worker to a core, but this can be
/// ignored if desired, of course. Each worker type will be assigned at least one core.
pub fn assign_workers(
    mut core_ids: Vec<CoreId>,
    assignments: Vec<(WorkerType, f64)>,
    log_core_ranges: bool, // Enable to log the range of cores each worker should be pinned to.
) -> (Vec<(core_affinity::CoreId, WorkerType)>, HashMap<WorkerType, u64>) {
    let mut result = Vec::new();
    let mut worker_counts: HashMap<WorkerType, u64> = HashMap::new();
    let mut worker_cores: HashMap<WorkerType, Vec<CoreId>> = HashMap::new();

    let total_starting_cores = core_ids.len();

    // First pass: assign at least one core to each worker type
    let mut remaining_cores = total_starting_cores;
    for (worker_type, _) in &assignments {
        if remaining_cores > 0 {
            if let Some(core_id) = core_ids.pop() {
                result.push((core_id, *worker_type));
                *worker_counts.entry(*worker_type).or_insert(0) += 1;
                worker_cores.entry(*worker_type).or_insert_with(Vec::new).push(core_id);
                remaining_cores -= 1;
            }
        }
    }

    // Second pass: distribute remaining cores based on percentages
    for (worker_type, percentage) in assignments {
        let theoritical_cores_for_type = (total_starting_cores as f64 * percentage).ceil() as usize;
        // Subtract the 1 core we already assigned
        let additional_cores_needed = theoritical_cores_for_type.saturating_sub(1);
        let actual_additional_cores = additional_cores_needed.min(remaining_cores);

        for _ in 0..actual_additional_cores {
            if let Some(core_id) = core_ids.pop() {
                result.push((core_id, worker_type));
                *worker_counts.entry(worker_type).or_insert(0) += 1;
                worker_cores.entry(worker_type).or_insert_with(Vec::new).push(core_id);
                remaining_cores -= 1;
            }
        }
    }

    println!("[+] Spawning {} workers:", total_starting_cores);
    for (worker_type, count) in worker_counts.clone() {
        if log_core_ranges {
            if let Some(cores) = worker_cores.get(&worker_type) {
                let mut core_ids: Vec<usize> = cores.iter().map(|c| c.id).collect();
                core_ids.sort();

                let core_str = match core_ids.as_slice() {
                    [single] => format!("core {}", single),
                    ids => {
                        let ranges = format_ranges(ids);
                        format!("cores {}", ranges)
                    }
                };
                println!("- {:?}: {} ({})", worker_type, count, core_str);
            }
        } else {
            println!("- {:?}: {}", worker_type, count);
        }
    }

    (result, worker_counts)
}

/// Format a sorted list of numbers into a range string (e.g., "1-3, 5, 7-9")
fn format_ranges(nums: &[usize]) -> String {
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
