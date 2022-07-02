//! Api module
//!

#[allow(unused_imports)]
use reqwest::Url;
use tracing::{
    info,
    debug,
    trace,
    warn,
    error
};
use tokio::sync::{
    mpsc,
    oneshot,
    Mutex
};
use warp::{
    filters::body::BodyDeserializeError,
    reject,
    reply,
    http::StatusCode,
    Filter,
    Rejection,
    Reply
};

use std::{
    convert::Infallible,
    net::IpAddr,
    sync::Arc
};

use crate::{
    errors::{
        EmptyRequest,
        InvalidUrl,
        SyncError
    },
    messages::{
        RequestMessage,
        StatusRequestMessage
    }
};

#[tracing::instrument(level="debug")]
async fn health_check() -> Result<impl Reply, Rejection> {
    debug!("Healthcheck requested");
    Ok(reply::with_status(
            String::from("OK"),
            StatusCode::OK
    ))
}

#[tracing::instrument(level="debug")]
async fn request_inspection(
    manager_tx: mpsc::Sender<RequestMessage>,
    list: Vec<String>
) -> Result<impl Reply, Rejection> {
    if list.is_empty() {
        warn!("Received request with 0 URLs");
        return Err(reject::custom(EmptyRequest));
    }

    // Assert that all of them are URLs
    let mut good_urls = Vec::new();
    for maybe_url in list {
        if let Ok(url) = Url::parse(&maybe_url) {
            good_urls.push(url);
        } else {
            warn!("Found invalid URL \"{}\" in request", maybe_url);
            return Err(reject::custom(InvalidUrl::new(maybe_url)));
        }
    }

    // Create a oneshot channel to receive the result
    let (ret_tx, ret_rx) = oneshot::channel();
    let reqmsg = RequestMessage::new(good_urls, ret_tx);
    manager_tx.send(reqmsg).await
        .map_err(|e| reject::custom(
                SyncError::from(e)
        ))?;

    let new_uuid = ret_rx.await
        .map_err(|e| reject::custom(
                SyncError::from(e)
        ))?;

    Ok(reply::with_status(
        new_uuid.to_string(),
        StatusCode::OK
    ))
}

#[tracing::instrument(level="debug")]
async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    if err.is_not_found() {
        Ok(reply::with_status("NOT_FOUND".into(), StatusCode::NOT_FOUND))
    } else if err.find::<EmptyRequest>().is_some() {
        Ok(reply::with_status("Empty Request".into(), StatusCode::BAD_REQUEST))
    } else if let Some(e) = err.find::<InvalidUrl>() {
        Ok(reply::with_status(format!("Invalid URL: \"{}\"", e.get_url()), StatusCode::BAD_REQUEST))
    } else if let Some(e) = err.find::<BodyDeserializeError>() {
        Ok(reply::with_status(format!("Deserialize error : {}", e), StatusCode::BAD_REQUEST))
    } else if let Some(e) = err.find::<SyncError<oneshot::error::RecvError>>() {
        Ok(reply::with_status(format!("Synchronization error : {:?}", e), StatusCode::INTERNAL_SERVER_ERROR))
    } else {
        warn!("Unhandled rejection: {:?}", err);
        Ok(reply::with_status("INTERNAL_SERVER_ERROR".into(), StatusCode::INTERNAL_SERVER_ERROR))
    }
}

#[tracing::instrument(level="debug")]
pub async fn start_api(
    manager_req_tx: mpsc::Sender<RequestMessage>,
    manager_poll_tx: mpsc::Sender<StatusRequestMessage>
) -> Result<(), Box<dyn std::error::Error + Send>> {
    // Turn the queues into filters
    let manager_req_tx = warp::any().map(move || manager_req_tx.clone());
    let _manager_poll_tx = warp::any().map(move || manager_poll_tx.clone());
    debug!("Composing API");

    let healthcheck = warp::path!("healthcheck")
        .and_then(health_check);
    debug!("Registered /healthcheck route");

    let new_request = warp::path!("request" / "new")
        .and(manager_req_tx.clone())
        .and(warp::body::json())
        .and_then(request_inspection);

    let routes = healthcheck
        .or(new_request)
        .recover(handle_rejection);

    let ip = std::env::var("IP")
        .ok()
        .and_then(|s| s.parse::<IpAddr>().ok())
        .unwrap_or_else(|| [127, 0, 0, 1].into());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(3008);

    info!("Launching at {}:{}", ip, port);
    warp::serve(routes).run((ip, port)).await;
    Ok(())
}
