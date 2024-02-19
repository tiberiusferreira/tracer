use std::backtrace::Backtrace;
use thiserror::__private::AsDynError;

pub fn error_chain_to_pretty_formatted<E>(error: E) -> String
where
    E: std::error::Error,
{
    let mut error = error.as_dyn_error();
    let mut err = format!("{}", error);
    while let Some(inner_err) = error.source() {
        err.push_str(&format!("\nCaused by: \n{}", inner_err));
        error = inner_err;
    }
    err
}

#[derive(Debug, thiserror::Error)]
#[error("SqlxError Context: {context}\n{backtrace}")]
pub struct SqlxError {
    #[source]
    pub source: sqlx::Error,
    pub context: String,
    pub backtrace: OptionBacktracePrettyPrinter,
}

#[derive(Debug, thiserror::Error)]
#[error("SerdeJsonError Context: {context}\n{bad_input_sample}\n{backtrace}")]
pub struct SerdeJsonError {
    #[source]
    pub source: serde_json::Error,
    pub context: String,
    pub bad_input_sample: String,
    pub backtrace: OptionBacktracePrettyPrinter,
}

impl SerdeJsonError {
    pub fn from_serde_json_error<S: Into<String>>(
        source: serde_json::Error,
        context: S,
        bad_input_sample: String,
    ) -> Self {
        Self {
            source,
            context: context.into(),
            bad_input_sample,
            backtrace: OptionBacktracePrettyPrinter::from(Backtrace::capture()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("ReqwestError Context: {context}\n{backtrace}")]
pub struct ReqwestError {
    #[source]
    pub source: reqwest::Error,
    pub context: String,
    pub backtrace: OptionBacktracePrettyPrinter,
}

impl ReqwestError {
    pub fn from_reqwest_error<S: Into<String>>(source: reqwest::Error, context: S) -> Self {
        Self {
            source,
            context: context.into(),
            backtrace: OptionBacktracePrettyPrinter::from(Backtrace::capture()),
        }
    }
}

#[derive(Debug)]
pub struct OptionBacktracePrettyPrinter(pub Option<Backtrace>);

impl OptionBacktracePrettyPrinter {
    pub fn capture() -> Self {
        Self::from(Backtrace::capture())
    }
}

impl From<Backtrace> for OptionBacktracePrettyPrinter {
    fn from(value: Backtrace) -> Self {
        Self(Some(value))
    }
}
impl std::fmt::Display for OptionBacktracePrettyPrinter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.as_ref() {
            Some(child) => write!(f, "{}", child),
            None => write!(f, "No backtrace"),
        }
    }
}

impl SqlxError {
    pub fn from_sqlx_error<S: Into<String>>(source: sqlx::Error, context: S) -> Self {
        Self {
            source,
            context: context.into(),
            backtrace: OptionBacktracePrettyPrinter::from(Backtrace::capture()),
        }
    }
}
