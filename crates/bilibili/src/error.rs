use reqwest::{Error as ReqwestError, StatusCode};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("网络请求失败：{0}")]
    Http(#[source] ReqwestError),
    #[error("响应解析失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error("B 站接口错误 {code}：{message}")]
    Bilibili { code: i64, message: String },
    #[error("HTTP 状态错误：{0}")]
    Status(StatusCode),
    #[error("响应格式异常：{0}")]
    InvalidResponse(String),
}

impl ApiError {
    pub fn is_login_required(&self) -> bool {
        matches!(self, Self::Bilibili { code: -101, .. })
    }

    pub fn is_risk_control(&self) -> bool {
        matches!(
            self,
            Self::Bilibili {
                code: -352 | -412,
                ..
            }
        ) || matches!(self, Self::Status(status) if status.as_u16() == 412)
    }
}

impl From<ReqwestError> for ApiError {
    fn from(error: ReqwestError) -> Self {
        Self::Http(error.without_url())
    }
}
