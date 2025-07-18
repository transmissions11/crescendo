use std::sync::Mutex;

pub struct PayloadQueue {
    queue: Mutex<Vec<Vec<u8>>>,
}

pub static PAYLOAD_QUEUE: PayloadQueue = PayloadQueue { queue: Mutex::new(Vec::new()) };

impl PayloadQueue {
    pub fn push_payload(&self, payload: Vec<u8>) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.push(payload);
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
            println!("[*] QGPS: {}, Total queue length: {}", queue_growth, current_queue_len);
            last_queue_len = current_queue_len;
        }
    }
}
