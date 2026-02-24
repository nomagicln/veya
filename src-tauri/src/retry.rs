use std::future::Future;
use std::time::Duration;

use crate::error::VeyaError;

pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl RetryPolicy {
    pub fn new(max_retries: u32, base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            max_retries,
            base_delay_ms,
            max_delay_ms,
        }
    }

    /// Execute an async operation with exponential backoff retry.
    ///
    /// The operation is called once initially, then up to `max_retries` additional
    /// times if it returns a retryable error. Non-retryable errors are returned
    /// immediately. Total calls on persistent failure = max_retries + 1.
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T, VeyaError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, VeyaError>>,
    {
        let mut last_error: Option<VeyaError> = None;

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if e.is_retryable() && attempt < self.max_retries {
                        let delay = std::cmp::min(
                            self.base_delay_ms.saturating_mul(2u64.saturating_pow(attempt)),
                            self.max_delay_ms,
                        );
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }
}
