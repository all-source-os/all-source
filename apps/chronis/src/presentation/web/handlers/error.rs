use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::domain::error::ChronError;

pub struct AppError(ChronError);

impl From<ChronError> for AppError {
    fn from(e: ChronError) -> Self {
        Self(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self.0 {
            ChronError::TaskNotFound(_) => (StatusCode::NOT_FOUND, self.0.to_string()),
            ChronError::InvalidTransition { .. }
            | ChronError::AlreadyDone(_)
            | ChronError::NotClaimed(_) => (StatusCode::CONFLICT, self.0.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()),
        };
        (status, msg).into_response()
    }
}
