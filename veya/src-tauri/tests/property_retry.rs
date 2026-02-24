// Feature: veya-mvp, Property 16: 重试策略执行
//
// For any configured retry count N and a persistently failing retryable operation,
// the system should execute exactly N retries, for a total of N+1 calls
// (1 initial + N retries).
//
// Validates: Requirement 8.1

use proptest::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use veya_lib::error::VeyaError;
use veya_lib::retry::RetryPolicy;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    #[test]
    fn retry_policy_executes_correct_count(retry_count in 1u32..10) {
        // Use a single-threaded tokio runtime with paused time so sleeps resolve instantly.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();

        rt.block_on(async {
            tokio::time::pause();

            let call_count = Arc::new(AtomicU32::new(0));
            let policy = RetryPolicy::new(retry_count, 100, 5000);

            let counter = call_count.clone();
            let result = policy
                .execute(|| {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Err::<(), VeyaError>(VeyaError::NetworkTimeout("test".into()))
                    }
                })
                .await;

            // Should have failed after all retries exhausted.
            assert!(result.is_err());

            // Total calls = 1 initial + N retries = N + 1
            let total_calls = call_count.load(Ordering::SeqCst);
            prop_assert_eq!(
                total_calls,
                retry_count + 1,
                "Expected {} calls (1 initial + {} retries), got {}",
                retry_count + 1,
                retry_count,
                total_calls
            );

            Ok(())
        })?;
    }
}
