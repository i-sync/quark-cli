use thiserror::Error;

pub type Result<T> = std::result::Result<T, QuarkPanError>;

#[derive(Debug, Error)]
pub enum QuarkPanError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("operation cancelled")]
    Cancelled,

    #[error("remote api error: status={status}, message={message}")]
    Api { status: u32, message: String },

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("header value error: {0}")]
    HeaderValue(#[from] reqwest::header::InvalidHeaderValue),

    #[error("url parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RetryClass {
    Transient,
    RateLimited,
    Hard,
}

impl QuarkPanError {
    pub fn missing_field(field: &'static str) -> Self {
        Self::MissingField(field)
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::InvalidArgument(message.into())
    }

    pub fn retry_class(&self) -> RetryClass {
        match self {
            Self::MissingField(_) | Self::InvalidArgument(_) | Self::Cancelled => RetryClass::Hard,
            Self::Api { status, .. } if *status == 429 => RetryClass::RateLimited,
            Self::Api { status, .. } if *status >= 500 => RetryClass::Transient,
            Self::Api { status, .. } if matches!(*status, 401 | 403 | 404) => RetryClass::Hard,
            Self::Api { .. } => RetryClass::Hard,
            Self::Http(err) if err.is_timeout() || err.is_connect() => RetryClass::Transient,
            Self::Http(err) if err.status().is_some_and(|status| status.as_u16() == 429) => {
                RetryClass::RateLimited
            }
            Self::Http(err) if err.status().is_some_and(|status| status.is_server_error()) => {
                RetryClass::Transient
            }
            Self::Http(err) if err.is_body() || err.is_request() => RetryClass::Transient,
            Self::Http(_) => RetryClass::Hard,
            Self::Io(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                RetryClass::Transient
            }
            Self::Io(_) => RetryClass::Hard,
            Self::Serde(_) | Self::HeaderValue(_) | Self::UrlParse(_) => RetryClass::Hard,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_class_treats_argument_errors_as_hard() {
        assert_eq!(
            QuarkPanError::missing_field("fid").retry_class(),
            RetryClass::Hard
        );
        assert_eq!(
            QuarkPanError::invalid_argument("bad").retry_class(),
            RetryClass::Hard
        );
    }

    #[test]
    fn retry_class_treats_cancelled_as_hard() {
        assert_eq!(QuarkPanError::Cancelled.retry_class(), RetryClass::Hard);
    }

    #[test]
    fn retry_class_treats_api_auth_and_not_found_as_hard() {
        for status in [401, 403, 404] {
            assert_eq!(
                QuarkPanError::Api {
                    status,
                    message: "api error".to_string()
                }
                .retry_class(),
                RetryClass::Hard
            );
        }
    }

    #[test]
    fn retry_class_treats_api_rate_limit_separately() {
        assert_eq!(
            QuarkPanError::Api {
                status: 429,
                message: "rate limited".to_string()
            }
            .retry_class(),
            RetryClass::RateLimited
        );
    }

    #[test]
    fn retry_class_treats_api_5xx_as_transient() {
        assert_eq!(
            QuarkPanError::Api {
                status: 503,
                message: "unavailable".to_string()
            }
            .retry_class(),
            RetryClass::Transient
        );
    }

    #[test]
    fn retry_class_treats_interrupted_io_as_transient() {
        let err = std::io::Error::new(std::io::ErrorKind::Interrupted, "interrupted");

        assert_eq!(QuarkPanError::Io(err).retry_class(), RetryClass::Transient);
    }
}
