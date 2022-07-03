use reqwest::Url;
use tokio::sync::oneshot;
use uuid::Uuid;
use warp::http::StatusCode;

use std::collections::HashMap;

#[derive(Debug, Copy, Clone)]
pub enum DownloadResult {
    Fetched(StatusCode),
    RedirectError,
    TimeOutError,
    RequestError,
    ConnectError,
    DecodeError,
    UnknownError
}

impl From<DownloadResult> for i32 {
    fn from(d: DownloadResult) -> Self {
        match d {
            DownloadResult::Fetched(s) => s.as_u16().into(),
            DownloadResult::RedirectError => -1,
            DownloadResult::TimeOutError => -2,
            DownloadResult::RequestError => -3,
            DownloadResult::ConnectError => -4,
            DownloadResult::DecodeError => -5,
            DownloadResult::UnknownError => -6
        }
    }
}

#[derive(Debug)]
pub struct RequestMessage {
    urls: Vec<Url>,
    result_tx: oneshot::Sender<Uuid>
}

impl RequestMessage {
    pub fn new(
        urls: Vec<Url>,
        result_tx: oneshot::Sender<Uuid>
    ) -> Self {
        Self { urls, result_tx }
    }

    // it's a destructor, it's not missing const : it can't be
    #[allow(clippy::missing_const_for_fn)]
    pub fn explode(self) -> (Vec<Url>, oneshot::Sender<Uuid>) {
        (self.urls, self.result_tx)
    }
}

#[derive(Debug)]
pub struct StatusReplyMessage {
    finished: bool,
    results: HashMap<Url, DownloadResult>
}

impl StatusReplyMessage {
    pub const fn new(finished: bool, results: HashMap<Url, DownloadResult>) -> Self {
        Self { finished, results }
    }
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn into_results(self) -> HashMap<Url, DownloadResult> {
        self.results
    }

    pub fn len(&self) -> usize {
        self.results.len()
    }
}

pub type StatusRequestMessage = (Uuid, oneshot::Sender<Option<StatusReplyMessage>>);

pub type SingleUrlDownload = (Uuid, Url);

pub type SingleUrlResult = (Uuid, Url, DownloadResult);
