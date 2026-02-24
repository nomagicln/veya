use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize, Clone)]
pub enum VeyaError {
    #[error("API Key 无效: {0}")]
    InvalidApiKey(String),

    #[error("余额不足: {0}")]
    InsufficientBalance(String),

    #[error("网络超时: {0}")]
    NetworkTimeout(String),

    #[error("模型不可用: {0}")]
    ModelUnavailable(String),

    #[error("OCR 识别失败: {0}")]
    OcrFailed(String),

    #[error("TTS 生成失败: {0}")]
    TtsFailed(String),

    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("系统权限不足: {0}")]
    PermissionDenied(String),

    #[error("{0}")]
    Generic(String),
}

impl VeyaError {
    /// Returns true if this error type is eligible for automatic retry.
    /// NetworkTimeout, ModelUnavailable, and TtsFailed are retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            VeyaError::NetworkTimeout(_) | VeyaError::ModelUnavailable(_) | VeyaError::TtsFailed(_)
        )
    }
}
