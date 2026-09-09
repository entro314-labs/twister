//! One error type for every Tauri command.
//!
//! Serializes as `"[CODE] message"`. The code half is a contract with the
//! renderer: `lib/query/client.ts` refuses to retry `INVALID_INPUT` and
//! `NOT_FOUND`, because neither fixes itself.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The caller sent something the app can reject without asking anyone.
    #[error("[INVALID_INPUT] {0}")]
    InvalidInput(String),
    /// The window or webview the command needs is not there.
    #[error("[NOT_FOUND] {0}")]
    NotFound(String),
    /// Ours: a broken settings file, a webview that refused an instruction.
    #[error("[INTERNAL] {0}")]
    Internal(String),
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(err: tauri::Error) -> Self {
        Self::Internal(format!("Window system error: {err}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::Internal(format!("Malformed JSON: {err}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(format!("File error: {err}"))
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

/// `Display` for a boxed cause, used where a third-party error has no `From`.
pub fn internal(context: &str, cause: impl fmt::Display) -> AppError {
    AppError::Internal(format!("{context}: {cause}"))
}
