use crate::{config::RequestLimits, error::AppError};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, SemaphorePermit};

pub(super) struct RequestGate {
    concurrent: Semaphore,
    bucket: Mutex<Bucket>,
    limits: RequestLimits,
}
struct Bucket {
    tokens: f64,
    updated: Instant,
}
impl RequestGate {
    pub fn new(limits: RequestLimits) -> Self {
        Self {
            concurrent: Semaphore::new(limits.max_concurrent),
            bucket: Mutex::new(Bucket {
                tokens: limits.burst as f64,
                updated: Instant::now(),
            }),
            limits,
        }
    }
    pub fn enter(&self) -> Result<SemaphorePermit<'_>, AppError> {
        let permit = self
            .concurrent
            .try_acquire()
            .map_err(|_| AppError::ApiBusy)?;
        let mut bucket = self.bucket.lock().expect("request bucket mutex poisoned");
        let now = Instant::now();
        bucket.tokens = (bucket.tokens
            + now.duration_since(bucket.updated).as_secs_f64() * self.limits.per_second as f64)
            .min(self.limits.burst as f64);
        bucket.updated = now;
        if bucket.tokens < 1.0 {
            return Err(AppError::ApiRateLimited);
        }
        bucket.tokens -= 1.0;
        Ok(permit)
    }
    pub fn timeout(&self) -> Duration {
        self.limits.timeout
    }
}
