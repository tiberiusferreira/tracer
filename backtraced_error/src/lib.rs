use std::backtrace::Backtrace;

pub fn error_to_pretty_formatted<T: std::error::Error + Send + Sync + 'static>(e: T) -> String {
    let err = anyhow::Error::new(e);
    format!("{err:?}")
}

#[derive(Debug, thiserror::Error)]
#[error("Sqlx error. Context: {context}\n{backtrace}")]
pub struct SqlxError {
    #[source]
    pub source: sqlx::Error,
    pub context: String,
    pub backtrace: OptionBacktracePrettyPrinter,
}

#[derive(Debug, thiserror::Error)]
#[error("Serde error. Context: {context}\n{bad_input_sample}\n{backtrace}")]
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
#[error("Reqwest error. Context: {context}\n{backtrace}")]
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
