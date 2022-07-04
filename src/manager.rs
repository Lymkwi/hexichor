//! Fetcher manager

use reqwest::Url;
use tracing::{
    error,
    info,
    warn
};
use tokio::{
    sync::{
        broadcast,
        mpsc
    },
    task::JoinHandle
};
use uuid::Uuid;

use std::collections::{
    hash_map::Entry,
    HashMap
};

use crate::messages::{
    DownloadResult,
    RequestMessage,
    SingleUrlDownload,
    SingleUrlResult,
    StatusRequestMessage,
    StatusReplyMessage
};

#[derive(Debug)]
pub struct Request {
    urls: HashMap<Url, Option<DownloadResult>>,
    remaining: usize
}

impl Request {
    fn update(&mut self, url: Url, res: DownloadResult) {
        match self.urls.entry(url) {
            Entry::Vacant(_) => { /* do nothing */ }
            Entry::Occupied(mut e) => {
                self.remaining -= 1;
                e.insert(Some(res));
            }
        }
    }

    fn fetch_done(&mut self) -> StatusReplyMessage {
        let mut done: HashMap<Url, DownloadResult> = HashMap::new();
        for entry in &self.urls {
            if let Some(val) = entry.1 {
                done.insert(entry.0.clone(), *val);
            }
        }
        for key in done.keys() {
            self.urls.remove(key);
        }
        
        StatusReplyMessage::new(self.remaining == 0, done)
    }
}

impl From<Vec<Url>> for Request {
    fn from(vc: Vec<Url>) -> Self {
        let len = vc.len();
        Self {
            urls: vc.into_iter()
                .map(|k| (k, None))
                .collect::<HashMap<Url, Option<DownloadResult>>>(),
            remaining: len
        }
    }
}

#[derive(Debug)]
pub struct Manager {
    reqs: HashMap<Uuid, Request>,
    workers: Vec<JoinHandle<()>>,
    dispatch_tx: async_channel::Sender<SingleUrlDownload>
}

impl Manager {
    pub fn new(ret_tx: &mpsc::Sender<SingleUrlResult>, wcount: usize) -> Self {
        // Channels
        let (sg_tx, sg_rx) = async_channel::unbounded();
        let mut workers = Vec::new();
        for i in 0..wcount {
            let new_rx = sg_rx.clone();
            let new_tx = ret_tx.clone();
            let handler = tokio::spawn(async move {
                if worker(i, new_rx, new_tx).await.is_err() {
                    error!("Worker {} terminated", i);
                }
            });
            workers.push(handler);
        }
        Self {
            reqs: HashMap::new(),
            workers,
            dispatch_tx: sg_tx
        }
    }

    fn collect(&mut self, uuid: Uuid) -> Option<StatusReplyMessage> {
        match self.reqs.entry(uuid) {
            Entry::Vacant(_) => { None },
            Entry::Occupied(mut entry) => {
                let statusreply = entry.get_mut().fetch_done();
                if statusreply.is_finished() {
                    info!("Removing finished entry {}", uuid);
                    entry.remove();
                }
                Some(statusreply)
            }
        }
    }

    fn set_result(&mut self, uuid: Uuid, url: Url, res: DownloadResult) {
        // Find entry in the dictionary
        if let Some(inner) = self.reqs.get_mut(&uuid) {
            inner.update(url, res);
        }
    }

    pub async fn register(&mut self, urls: Vec<Url>) -> Uuid {
        // Generate UUID
        let mut key = Uuid::new_v4();
        while self.reqs.contains_key(&key) {
            key = Uuid::new_v4();
        }
        info!("Registered new request for {} URLs with UUID={}",
              urls.len(),
              key
        );
        for url in &urls {
            self.dispatch_tx.send((key, url.clone())).await.unwrap();
        }
        self.reqs.insert(key, Request::from(urls));
        key
    }

    async fn shutdown(self) {
        std::mem::drop(self.dispatch_tx);
        for handle in self.workers {
            std::mem::drop(handle.await);
        }
    }
}

#[tracing::instrument(level="info", skip(req_rx,poll_rx, shutdown_rx))]
pub async fn manager(
    mut req_rx: mpsc::Receiver<RequestMessage>,
    mut poll_rx: mpsc::Receiver<StatusRequestMessage>,
    mut shutdown_rx: broadcast::Receiver<()>
) -> Result<(), Box<dyn std::error::Error + Send>> {
    let (ret_tx, mut ret_rx) = mpsc::channel(128);
    let mut data = Manager::new(&ret_tx, 5);
    loop {
        tokio::select! {
            Some(reqmsg) = req_rx.recv() => {
                //info!("I got a reqmsg: {:?}", reqmsg);
                // Explode the request
                let (urls, ret_tx) = reqmsg.explode();
                // Register the request and respond
                if ret_tx.send(data.register(urls).await).is_err() {
                    error!("Unable to send back addition result");
                }
            }
            Some(statusreqmsg) = poll_rx.recv() => {
                //info!("I got a statusreqmsg: {:?}", statusreqmsg);
                // Explode
                let (uuid, o_tx) = statusreqmsg;
                // Collect data
                // Build result
                match data.collect(uuid) {
                    None => if o_tx.send(None).is_err() {
                        warn!("Unable to send useless request status result");
                    },
                    Some(data) => {
                        let len = data.len();
                        if o_tx.send(Some(data)).is_err() {
                            error!("Unable to send request status result (dropped {} results)", len);
                        }
                    }
                }
            }
            Some(result) = ret_rx.recv() => {
                let (uuid, url, res) = result;
                data.set_result(uuid, url, res);
            }
            Ok(()) = shutdown_rx.recv() => {
                break;
            }
            else => {
                /* One of the channels was stopped, i.e.
                 * the API was stopped or crashed. Quit.
                 */
                break;
            }
        }
    }
    data.shutdown().await;
    info!("Workers shut down");
    Ok(())
}

#[tracing::instrument(level="debug", skip(id, order_rx, return_tx))]
async fn worker(
    id: usize,
    order_rx: async_channel::Receiver<SingleUrlDownload>,
    return_tx: mpsc::Sender<SingleUrlResult>
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match order_rx.recv().await {
            Err(_) => { break },
            Ok(request) => {
                let (uuid, url) = request;
                info!("Worker {} got ({}):{}", id, uuid, url.to_string());
                // Actually fetch
                let value = match reqwest::get(url.clone()).await {
                    Ok(rs) => DownloadResult::Fetched(rs.status()),
                    Err(err) => {
                        err.status().map_or_else(|| 
                            if err.is_redirect() {
                                DownloadResult::RedirectError
                            } else if err.is_timeout() {
                                DownloadResult::TimeOutError
                            } else if err.is_request() {
                                DownloadResult::RequestError
                            } else if err.is_connect() {
                                DownloadResult::ConnectError
                            } else if err.is_decode() {
                                DownloadResult::DecodeError
                            } else {
                                DownloadResult::UnknownError
                            }
                        , DownloadResult::Fetched)
                    }
                };

                info!("Obtained ({}):{} => {:?}", uuid, url.to_string(), value);
                // Build result
                let result = (uuid, url, value);
                if let Err(e) = return_tx.send(result).await {
                    error!("Could not return fetch result : {}", e);
                }
            }
        }
    }
    info!("Worker {} terminated", id);
    Ok(())
}
