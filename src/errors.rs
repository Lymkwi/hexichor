use warp::reject;

#[derive(Debug)]
pub struct EmptyRequest;

impl reject::Reject for EmptyRequest {}

#[derive(Debug)]
pub struct InvalidUrl {
    url: String
}
impl InvalidUrl {
    pub const fn new(st: String) -> Self {
        Self { url: st }
    }

    pub fn get_url(&self) -> &str {
        &self.url
    }
}
impl reject::Reject for InvalidUrl {}

#[derive(Debug)]
pub struct SyncError<E> {
    err: E
}

impl<E: std::error::Error + Send + Sync + 'static + Sized> SyncError<E> {
    pub const fn from(err: E) -> Self {
        Self { err }
    }
}

impl<E: std::error::Error + Send + Sync + 'static + std::fmt::Debug> SyncError<E> {
    pub fn get_error(&self) -> &E {
        &self.err
    }
}

impl<E: std::error::Error + Send + Sync + 'static> reject::Reject for SyncError<E> {}

#[derive(Debug)]
pub struct Unauthorized;

impl reject::Reject for Unauthorized {}
