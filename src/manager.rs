//! Fetcher manager

use uuid::Uuid;
use reqwest::Url;
use tracing::{
    info,
    error
};
use tokio::{
    sync::{
        mpsc
    }
};
use warp::http::StatusCode;

use std::collections::HashMap;

use crate::messages::{
    RequestMessage,
    StatusRequestMessage
};

#[derive(Debug)]
pub struct Request {
    urls: HashMap<Url, Option<StatusCode>>,
    remaining: usize
}

impl From<Vec<Url>> for Request {
    fn from(vc: Vec<Url>) -> Self {
        let len = vc.len();
        Self {
            urls: vc.into_iter()
                .map(|k| (k, None))
                .collect::<HashMap<Url, Option<StatusCode>>>(),
            remaining: len
        }
    }
}

#[derive(Debug, Default)]
pub struct Manager {
    reqs: HashMap<Uuid, Request>
}

impl Manager {
    pub fn register(&mut self, urls: Vec<Url>) -> Uuid {
        // Generate UUID
        let mut key = Uuid::new_v4();
        while self.reqs.contains_key(&key) {
            key = Uuid::new_v4();
        }
        info!("Registered new request for {} URLs with UUID={}",
              urls.len(),
              key
        );
        self.reqs.insert(key, Request::from(urls));
        key
    }
}

#[tracing::instrument(level="info", skip(req_rx,poll_rx))]
pub async fn manager(
    mut req_rx: mpsc::Receiver<RequestMessage>,
    mut poll_rx: mpsc::Receiver<StatusRequestMessage>
) -> Result<(), Box<dyn std::error::Error + Send>> {
    let mut data = Manager::default();
    loop {
        tokio::select! {
            Some(reqmsg) = req_rx.recv() => {
                //info!("I got a reqmsg: {:?}", reqmsg);
                // Explode the request
                let (urls, ret_tx) = reqmsg.explode();
                // Register the request and respond
                match ret_tx.send(data.register(urls)) {
                    Ok(()) => { /* all went ok */ }
                    Err(uuid) => {
                        error!("Unable to send back uuid {}", uuid);
                    }
                }
            }
            Some(statusreqmsg) = poll_rx.recv() => {
                info!("I got a statusreqmsg: {:?}", statusreqmsg);
            }
            else => {
                /* One of the channels was stopped, i.e.
                 * the API was stopped or crashed. Quit.
                 */
                return Ok(());
            }
        }
    }
}
