use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use ratelimit::Ratelimiter;
use thousands::Separable;

use crate::workers::NUM_ACCOUNTS;

const RATELIMIT_MIN: u64 = 1_000;
const RATELIMIT_MAX: u64 = 25_000;
const RATELIMIT_INCREASE_THRESHOLD: u64 = NUM_ACCOUNTS as u64 * 2;

pub struct TxQueue {
    // TODO: RwLock? Natively concurrent deque?
    queue: Mutex<VecDeque<Vec<u8>>>,
    total_added: AtomicU64,
    total_popped: AtomicU64,
    rate_limiter: Ratelimiter,
}

impl TxQueue {
    fn new() -> Self {
        // TODO: Configure bursting and other parameters?
        let rate_limiter = Ratelimiter::builder(
            RATELIMIT_MIN,          // Refill amount.
            Duration::from_secs(1), // Refill rate.
        )
        .max_tokens(RATELIMIT_MAX) // Burst limit.
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
        let max_count = queue.len().min(max_count);
        let allowed = (0..max_count).take_while(|_| self.rate_limiter.try_wait().is_ok()).count();
        if allowed == 0 {
            return None;
        };

        let popped = self.total_popped.fetch_add(allowed as u64, Ordering::Relaxed);

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
            let current_total_added = self.total_added.load(Ordering::Relaxed);
            let current_total_popped = self.total_popped.load(Ordering::Relaxed);
            let current_queue_len = self.queue_len();
            let added_per_second = (current_total_added - last_total_added) / measurement_interval.as_secs();
            let popped_per_second = (current_total_popped - last_total_popped) / measurement_interval.as_secs();
            let queue_growth =
                ((current_queue_len.saturating_sub(last_queue_len)) as u64) / measurement_interval.as_secs();

            // If we've popped more than RATELIMIT_INCREASE_THRESHOLD txs, increase the rate limit to the max.
            if current_total_popped > RATELIMIT_INCREASE_THRESHOLD && self.rate_limiter.max_tokens() < RATELIMIT_MAX {
                self.rate_limiter.set_refill_amount(RATELIMIT_MAX).unwrap();
            }

            println!(
                "[*] TxQueue +/s: {}, -/s: {}, Δ/s: {}, Length: {}, Rate limit: {}/s",
                added_per_second.separate_with_commas(),
                popped_per_second.separate_with_commas(),
                queue_growth.separate_with_commas(),
                current_queue_len.separate_with_commas(),
                self.rate_limiter.max_tokens().separate_with_commas()
            );

            last_total_added = current_total_added;
            last_total_popped = current_total_popped;
            last_queue_len = current_queue_len;
        }
    }
}
