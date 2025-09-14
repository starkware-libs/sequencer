use tracing::error;
use validator::ValidationError;

pub(crate) fn create_validation_error(
    error_msg: String,
    validate_code: &'static str,
    validate_error_msg: &'static str,
) -> ValidationError {
    error!(error_msg);
    let mut error = ValidationError::new(validate_code);
    error.message = Some(validate_error_msg.into());
    error
}
