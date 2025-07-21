use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use ratelimit::Ratelimiter;
use thousands::Separable;

use crate::workers::NUM_ACCOUNTS;

const INITIAL_RATELIMIT: u64 = 500;
#[rustfmt::skip]
const RATELIMIT_THRESHOLDS: [(u32, u64); 5] = [
    (NUM_ACCOUNTS / 4, 1_000),
    (NUM_ACCOUNTS / 2, 2_500),
    (NUM_ACCOUNTS,     5_000),
    (NUM_ACCOUNTS * 2, 10_000),
    (NUM_ACCOUNTS * 4, 25_000),
]; // Note: This must be sorted in ascending order of threshold!

pub struct TxQueue {
    // TODO: RwLock? Natively concurrent deque?
    queue: Mutex<VecDeque<Vec<u8>>>,
    total_added: AtomicU64,
    total_popped: AtomicU64,
    rate_limiter: Ratelimiter,
}

impl TxQueue {
    fn new() -> Self {
        // Modulates rate at which txs can be popped from the queue.
        let rate_limiter = Ratelimiter::builder(
            INITIAL_RATELIMIT,      // Refill amount.
            Duration::from_secs(1), // Refill rate.
        )
        .max_tokens(INITIAL_RATELIMIT) // Burst limit.
        .build()
        .unwrap();

        Self {
            queue: Mutex::new(VecDeque::new()),
            total_added: AtomicU64::new(0),
            total_popped: AtomicU64::new(0),
            rate_limiter,
        }
    }
}

pub static TX_QUEUE: std::sync::LazyLock<TxQueue> = std::sync::LazyLock::new(TxQueue::new);

impl TxQueue {
    pub fn push_tx(&self, tx: Vec<u8>) {
        self.total_added.fetch_add(1, Ordering::Relaxed);
        self.queue.lock().unwrap().push_back(tx);
    }

    pub fn queue_len(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    pub async fn pop_at_most(&self, max_count: usize) -> Option<Vec<Vec<u8>>> {
        let mut queue = self.queue.lock().ok()?;
        let allowed = (0..queue.len().min(max_count)).take_while(|_| self.rate_limiter.try_wait().is_ok()).count();
        if allowed == 0 {
            return None;
        };

        self.total_popped.fetch_add(allowed as u64, Ordering::Relaxed);

        Some(queue.drain(..allowed).collect())
    }

    pub async fn start_reporter(&self, measurement_interval: std::time::Duration) {
        let mut last_total_added = 0u64;
        let mut last_total_popped = 0u64;
        let mut last_queue_len = 0usize;
        let mut interval = tokio::time::interval(measurement_interval);
        interval.tick().await;
        loop {
            interval.tick().await;
            let total_added = self.total_added.load(Ordering::Relaxed);
            let total_popped = self.total_popped.load(Ordering::Relaxed);
            let current_queue_len = self.queue_len();
            let added_per_second = (total_added - last_total_added) / measurement_interval.as_secs();
            let popped_per_second = (total_popped - last_total_popped) / measurement_interval.as_secs();
            let queue_growth =
                ((current_queue_len.saturating_sub(last_queue_len)) as u64) / measurement_interval.as_secs();

            // Adjust rate limit based on total popped transactions and thresholds.
            let new_rate_limit = RATELIMIT_THRESHOLDS
                .iter()
                .rev()
                .find(|(threshold, _)| total_popped >= (*threshold as u64))
                .map(|(_, rate_limit)| *rate_limit)
                .unwrap_or(INITIAL_RATELIMIT);
            if self.rate_limiter.refill_amount() != new_rate_limit {
                println!("[+] Adjusting rate limit to {} txs/s", new_rate_limit.separate_with_commas());
                self.rate_limiter.set_max_tokens(new_rate_limit).unwrap(); // Burst limit must be set first.
                self.rate_limiter.set_refill_amount(new_rate_limit).unwrap();
            }

            println!(
                "[*] TxQueue +/s: {}, -/s: {}, Δ/s: {}, Length: {}, Rate limit: {}/s",
                added_per_second.separate_with_commas(),
                popped_per_second.separate_with_commas(),
                queue_growth.separate_with_commas(),
                current_queue_len.separate_with_commas(),
                self.rate_limiter.refill_amount().separate_with_commas()
            );

            last_total_added = total_added;
            last_total_popped = total_popped;
            last_queue_len = current_queue_len;
        }
    }
}
