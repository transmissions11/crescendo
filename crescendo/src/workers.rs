use std::collections::HashMap;

use core_affinity::CoreId;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum WorkerType {
    TxGen,
    Network,
}

pub fn assign_workers(
    mut core_ids: Vec<CoreId>,
    assignments: Vec<(WorkerType, f64)>,
) -> (Vec<(core_affinity::CoreId, WorkerType)>, HashMap<WorkerType, u64>) {
    let mut result = Vec::new();
    let mut worker_counts: HashMap<WorkerType, u64> = HashMap::new();

    let total_starting_cores = core_ids.len();

    let mut remaining_cores = total_starting_cores;
    for (worker_type, percentage) in assignments {
        let theoritical_cores_for_type = (total_starting_cores as f64 * percentage).ceil() as usize;
        let actual_cores_for_type = theoritical_cores_for_type.min(remaining_cores).max(1);

        for _ in 0..actual_cores_for_type {
            if let Some(core_id) = core_ids.pop() {
                result.push((core_id, worker_type));
                *worker_counts.entry(worker_type).or_insert(0) += 1;
                remaining_cores -= 1;
            }
        }
    }

    println!("Spawning {} workers:", total_starting_cores);
    for (worker_type, count) in worker_counts.clone() {
        println!("- {:?}: {}", worker_type, count);
    }

    (result, worker_counts)
}
