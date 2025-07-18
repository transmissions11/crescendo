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
) -> (Vec<(core_affinity::CoreId, WorkerType)>, HashMap<WorkerType, u64>) {
    let mut result = Vec::new();
    let mut worker_counts: HashMap<WorkerType, u64> = HashMap::new();

    let total_starting_cores = core_ids.len();

    // First pass: assign at least one core to each worker type
    let mut remaining_cores = total_starting_cores;
    for (worker_type, _) in &assignments {
        if remaining_cores > 0 {
            if let Some(core_id) = core_ids.pop() {
                result.push((core_id, *worker_type));
                *worker_counts.entry(*worker_type).or_insert(0) += 1;
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
                remaining_cores -= 1;
            }
        }
    }

    println!("[+] Spawning {} workers:", total_starting_cores);
    for (worker_type, count) in worker_counts.clone() {
        println!("- {:?}: {}", worker_type, count);
    }

    (result, worker_counts)
}
