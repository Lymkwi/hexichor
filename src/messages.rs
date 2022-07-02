use reqwest::Url;
use tokio::sync::oneshot;
use uuid::Uuid;
use warp::http::StatusCode;

use std::collections::HashMap;

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
    results: Option<HashMap<Url, StatusCode>>
}

impl StatusReplyMessage {
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    pub const fn get_results(&self) -> Option<&HashMap<Url, StatusCode>> {
        self.results.as_ref()
    }
}

#[derive(Debug)]
pub struct StatusRequestMessage {
    uuid: Uuid,
    channel: oneshot::Sender<StatusReplyMessage>
}
