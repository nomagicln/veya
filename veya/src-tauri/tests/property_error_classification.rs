// Feature: veya-mvp, Property 17: 错误类型分类
//
// For any API error variant (InvalidApiKey, InsufficientBalance, NetworkTimeout,
// ModelUnavailable, OcrFailed, TtsFailed, StorageError, PermissionDenied),
// after all retries are exhausted the returned error must preserve the concrete
// error type identifier so the frontend can display a meaningful message.
//
// Validates: Requirement 8.2

use proptest::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use veya_lib::error::VeyaError;
use veya_lib::retry::RetryPolicy;

/// Helper: build an arbitrary VeyaError from an index (0..8) and a detail string.
fn make_error(variant: u8, detail: String) -> VeyaError {
    match variant % 8 {
        0 => VeyaError::InvalidApiKey(detail),
        1 => VeyaError::InsufficientBalance(detail),
        2 => VeyaError::NetworkTimeout(detail),
        3 => VeyaError::ModelUnavailable(detail),
        4 => VeyaError::OcrFailed(detail),
        5 => VeyaError::TtsFailed(detail),
        6 => VeyaError::StorageError(detail),
        _ => VeyaError::PermissionDenied(detail),
    }
}

/// Return a discriminant tag for the error so we can compare identity.
fn error_tag(e: &VeyaError) -> &'static str {
    match e {
        VeyaError::InvalidApiKey(_) => "InvalidApiKey",
        VeyaError::InsufficientBalance(_) => "InsufficientBalance",
        VeyaError::NetworkTimeout(_) => "NetworkTimeout",
        VeyaError::ModelUnavailable(_) => "ModelUnavailable",
        VeyaError::OcrFailed(_) => "OcrFailed",
        VeyaError::TtsFailed(_) => "TtsFailed",
        VeyaError::StorageError(_) => "StorageError",
        VeyaError::PermissionDenied(_) => "PermissionDenied",
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// After all retries are exhausted the returned error must carry the same
    /// variant (type identifier) as the original error produced by the operation.
    #[test]
    fn error_type_preserved_after_retries(
        variant in 0u8..8,
        detail in "[a-zA-Z0-9 ]{1,30}",
        retry_count in 1u32..6,
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();

        rt.block_on(async {
            tokio::time::pause();

            let error_template = make_error(variant, detail);
            let expected_tag = error_tag(&error_template);
            let is_retryable = error_template.is_retryable();

            let call_count = Arc::new(AtomicU32::new(0));
            let err_clone = error_template.clone();
            let counter = call_count.clone();

            let policy = RetryPolicy::new(retry_count, 10, 500);

            let result: Result<(), VeyaError> = policy
                .execute(|| {
                    let counter = counter.clone();
                    let err = err_clone.clone();
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Err(err)
                    }
                })
                .await;

            // Must always fail.
            let returned_err = result.expect_err("operation should have failed");

            // Core property: the returned error type must match the original.
            let returned_tag = error_tag(&returned_err);
            prop_assert_eq!(
                returned_tag,
                expected_tag,
                "Error type changed after retries: expected {}, got {}",
                expected_tag,
                returned_tag
            );

            // Secondary: non-retryable errors should fail immediately (1 call),
            // retryable errors should exhaust all retries (retry_count + 1 calls).
            let total = call_count.load(Ordering::SeqCst);
            if is_retryable {
                prop_assert_eq!(
                    total,
                    retry_count + 1,
                    "Retryable error should exhaust all retries"
                );
            } else {
                prop_assert_eq!(
                    total,
                    1,
                    "Non-retryable error should fail on first call"
                );
            }

            Ok(())
        })?;
    }
}
