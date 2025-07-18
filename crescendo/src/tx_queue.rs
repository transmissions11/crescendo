use std::sync::Mutex;

use thousands::Separable;

pub struct TxQueue {
    queue: Mutex<Vec<Vec<u8>>>,
}

pub static TX_QUEUE: TxQueue = TxQueue { queue: Mutex::new(Vec::new()) };

impl TxQueue {
    pub fn push_tx(&self, tx: Vec<u8>) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.push(tx);
        }
    }

    pub fn queue_len(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    pub async fn start_reporter(&self, measurement_interval: std::time::Duration) {
        let mut last_queue_len = 0usize;
        let mut interval = tokio::time::interval(measurement_interval);
        interval.tick().await;
        loop {
            interval.tick().await;
            let current_queue_len = self.queue_len();
            let queue_growth = current_queue_len.saturating_sub(last_queue_len);
            println!(
                "[*] TxQueue Δ/s: {}, Total length: {}",
                queue_growth.separate_with_commas(),
                current_queue_len.separate_with_commas()
            );
            last_queue_len = current_queue_len;
        }
    }
}
