use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use thousands::Separable;

use crate::workers::NUM_ACCOUNTS;

const RAMP_UP_MIN_SLEEP_DURATION: std::time::Duration = std::time::Duration::from_secs(1);
const RAMP_UP_THRESHOLDS: [u64; 14] = [
    (NUM_ACCOUNTS / 100) as u64,
    (NUM_ACCOUNTS / 50) as u64,
    (NUM_ACCOUNTS / 25) as u64,
    (NUM_ACCOUNTS / 10) as u64,
    (NUM_ACCOUNTS / 5) as u64,
    (NUM_ACCOUNTS / 3) as u64,
    (NUM_ACCOUNTS / 2) as u64,
    (NUM_ACCOUNTS) as u64,
    (NUM_ACCOUNTS * 2) as u64,
    (NUM_ACCOUNTS * 5) as u64,
    (NUM_ACCOUNTS * 10) as u64,
    (NUM_ACCOUNTS * 20) as u64,
    (NUM_ACCOUNTS * 100) as u64,
    (NUM_ACCOUNTS * 1000) as u64,
];

pub struct TxQueue {
    // TODO: RwLock? Natively concurrent deque?
    queue: Mutex<VecDeque<Vec<u8>>>,
    total_added: AtomicU64,
    total_popped: AtomicU64,
    popping_paused: AtomicBool,
}

pub static TX_QUEUE: TxQueue = TxQueue {
    queue: Mutex::new(VecDeque::new()),
    total_added: AtomicU64::new(0),
    total_popped: AtomicU64::new(0),
    popping_paused: AtomicBool::new(false),
};

impl TxQueue {
    pub fn push_tx(&self, tx: Vec<u8>) {
        self.total_added.fetch_add(1, Ordering::Relaxed);
        self.queue.lock().unwrap().push_back(tx);
    }

    pub fn queue_len(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    pub async fn pop_at_most(&self, max_count: usize) -> Option<Vec<Vec<u8>>> {
        let (count, txs) = {
            if self.popping_paused.load(Ordering::Relaxed) {
                return None;
            }

            let mut queue = self.queue.lock().ok()?;

            let count = max_count.min(queue.len());
            if count == 0 {
                return None;
            }

            // TODO: Is drain more or less efficient than repeated pop_front?
            // It's important to pop from the front here, otherwise the node
            // gets confused seeing a bunch of txs with incredibly high nonces
            // before it sees any of the lower ones. It's possible this issue
            // could still emerge at high enough RPS, but haven't seen it yet.
            (count as u64, queue.drain(..count).collect())
        };

        // If we're going to cross a ramp-up threshold, sleep for a second.
        let prev_popped = self.total_popped.fetch_add(count, Ordering::Relaxed);
        for (i, &threshold) in RAMP_UP_THRESHOLDS.iter().enumerate() {
            if prev_popped < threshold && threshold <= (prev_popped + count) {
                let sleep_duration = RAMP_UP_BASE_SLEEP_DURATION * (i + 1) as u32; // Increase with each threshold.
                println!("[-] Pausing popping for {:.1?} at {} txs.", sleep_duration, threshold.separate_with_commas());
                self.popping_paused.store(true, Ordering::Relaxed); // Lock popping.
                tokio::time::sleep(sleep_duration).await;
                self.popping_paused.store(false, Ordering::Relaxed); // Unlock popping.
                println!("[+] Resuming popping.");
                break;
            }
        }

        Some(txs)
    }

    pub async fn start_reporter(&self, measurement_interval: std::time::Duration) {
        let mut last_total_added = 0u64;
        let mut last_queue_len = 0usize;
        let mut interval = tokio::time::interval(measurement_interval);
        interval.tick().await;
        loop {
            interval.tick().await;
            let current_total_added = self.total_added.load(Ordering::Relaxed);
            let current_queue_len = self.queue_len();
            let added_per_second = (current_total_added - last_total_added) / measurement_interval.as_secs();
            let queue_growth =
                ((current_queue_len.saturating_sub(last_queue_len)) as u64) / measurement_interval.as_secs();
            println!(
                "[*] TxQueue +/s: {}, TxQueue Δ/s: {}, Current length: {}",
                added_per_second.separate_with_commas(),
                queue_growth.separate_with_commas(),
                current_queue_len.separate_with_commas()
            );
            last_total_added = current_total_added;
            last_queue_len = current_queue_len;
        }
    }
}
