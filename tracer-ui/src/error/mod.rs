#[derive(Clone, Debug, thiserror::Error)]
#[error("Error at {location}")]
pub struct TrackedError<T: std::error::Error> {
    pub location: &'static std::panic::Location<'static>,
    #[source]
    pub source: T,
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("TrackedGlooError at {location}")]
pub struct TrackedGlooError {
    location: &'static std::panic::Location<'static>,
    #[source]
    source: std::sync::Arc<gloo_net::Error>,
}

impl From<gloo_net::Error> for TrackedGlooError {
    #[track_caller]
    fn from(err: gloo_net::Error) -> Self {
        Self {
            location: std::panic::Location::caller(),
            source: std::sync::Arc::new(err),
        }
    }
}
