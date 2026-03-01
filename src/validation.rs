use validator::Validate;

use crate::error::AppError;

pub fn validate_input<T: Validate>(input: &T) -> Result<(), AppError> {
    input
        .validate()
        .map_err(|e| AppError::Validation(e.to_string()))
}
