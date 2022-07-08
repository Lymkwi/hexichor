//! Api module
//!

#[allow(unused_imports)]
use reqwest::Url;
use tracing::{
	debug,
	error,
	info,
	warn,
};
use tokio::sync::{
	broadcast,
	mpsc,
	oneshot,
	Mutex
};
use uuid::Uuid;
use warp::{
	filters::body::BodyDeserializeError,
	reject,
	reply,
	http::{
		Response,
		StatusCode
	},
	Filter,
	Rejection,
	Reply
};

use std::{
	convert::Infallible,
	net::{
		SocketAddr,
		ToSocketAddrs
	},
	sync::Arc
};

use crate::{
	auth::Engine,
	dto::{
		StatusReply,
		LoginRequest
	},
	errors::{
		EmptyRequest,
		InvalidUrl,
		SyncError,
		Unauthorized
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
async fn request_status(
	uuid: Uuid,
	status_tx: mpsc::Sender<StatusRequestMessage>
) -> Result<impl warp::Reply, warp::Rejection> {
	// Status request
	let (o_tx, o_rx) = oneshot::channel();
	let req = (uuid, o_tx);
	status_tx.send(req).await
		.map_err(SyncError::from)?;
	match o_rx.await.map_err(SyncError::from)? {
		Some(result) => Ok({
			reply::json(& StatusReply::new(
					result.is_finished(),
					result.into_results()
						.into_iter()
						.map(|(k,v)| {
							(k.to_string(), i32::from(v))
						})
						.collect()
			))
		}),
		None => {
			warn!("Request missing UUID={}", uuid);
			Err(reject::reject())
		}
	}
}

fn check_authentication<E: Filter<Extract=(Arc<Mutex<Engine>>,), Error=Infallible> + Clone + Send + Sync>(
auth_engine: E
) -> impl Filter<Extract=((),), Error=Rejection> + Clone {
	warp::filters::cookie::optional::<String>("HEX")
		.and(auth_engine)
		.and_then(cookie_checker)
		//.map(|a: String, b: Arc<Mutex<Engine>>| {}) //cookie_checker)
}

async fn cookie_checker(
	cookie: Option<String>,
	engine: Arc<Mutex<Engine>>
) -> Result<(), Rejection> {
	let engine = engine.lock().await;
	cookie.map_or_else(|| Err(warp::reject::custom(Unauthorized)),
		|cook| if engine.is_authorized(&cook) {
			Ok(())
		} else {
			Err(warp::reject::custom(Unauthorized))		
	})
}

#[tracing::instrument(level="debug", skip(auth_engine, body))]
async fn authentication_request(
	auth_engine: Arc<Mutex<Engine>>,
	body: LoginRequest
) -> Result<impl warp::Reply, warp::Rejection> {
	// Try and get the login
	let mut engine = auth_engine.lock().await;
	// Try and authenticate
	match engine.verify(body.get_user(), body.get_password().as_bytes()) {
		None => {
			debug!("Failed authentication");
			Ok(Response::builder()
				.status(StatusCode::UNAUTHORIZED)
				.body("UNAUTHORIZED")
		)},
		Some(cookie) => {
			info!("Successful authentication of user {}", body.get_user());
			Ok(Response::builder()
				.status(StatusCode::OK)
				.header("set-cookie", format!("HEX={}", cookie))
				.body("OK")
			)
		}
	}
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
		Ok(reply::with_status(format!("Synchronization error : {:?}", e.get_error()), StatusCode::INTERNAL_SERVER_ERROR))
	} else if err.find::<Unauthorized>().is_some() {
		Ok(reply::with_status("UNAUTHORIZED".into(), StatusCode::UNAUTHORIZED))
	} else {
		warn!("Unhandled rejection: {:?}", err);
		Ok(reply::with_status("INTERNAL_SERVER_ERROR".into(), StatusCode::INTERNAL_SERVER_ERROR))
	}
}

#[tracing::instrument(level="debug")]
pub async fn start_api(
	bind_tuple: (&str, u16),
	manager_req_tx: mpsc::Sender<RequestMessage>,
	manager_poll_tx: mpsc::Sender<StatusRequestMessage>,
	mut shut_rx: broadcast::Receiver<()>
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	// Authentication engine
	let auth_engine = Engine::new("creds.yaml")
		.map_err(|e| {
			error!("Unable to start API auth engine! {}", e);
			e
		})?;
	let auth_engine = Arc::new(Mutex::new(auth_engine));
	let auth_engine = warp::any().map(move || Arc::clone(&auth_engine));
	// Turn the queues into filters
	let manager_req_tx = warp::any().map(move || manager_req_tx.clone());
	let manager_poll_tx = warp::any().map(move || manager_poll_tx.clone());
	debug!("Composing API");

	let healthcheck = warp::path!("healthcheck")
		.and_then(health_check);
	debug!("Registered /healthcheck route");

	let new_request = warp::path!("request" / "new")
		.and(check_authentication(auth_engine.clone()))
		.untuple_one()
		.and(manager_req_tx.clone())
		.and(warp::body::json())
		.and_then(request_inspection);
	debug!("Registered /request/new route");

	let status_request = warp::path("request")
		.and(check_authentication(auth_engine.clone()))
		.untuple_one()
		// Path param is moved into its own filter so that
		// it is not passed to `check_authentication`
		.and(warp::path::param())
		.and(manager_poll_tx.clone())
		.and_then(request_status);
	debug!("Registered /request/<uuid>");

	let login = warp::path!("login")
		.and(warp::filters::method::post())
		.and(auth_engine.clone())
		.and(warp::body::json())
		.and_then(authentication_request);
	debug!("Registered /login");

	let routes = healthcheck
		.or(new_request)
		.or(status_request)
		.or(login)
		.recover(handle_rejection);

	info!("Launching at {}:{}", bind_tuple.0, bind_tuple.1);
	let sock_tuple: SocketAddr = bind_tuple
		.to_socket_addrs()
		.map_err(|e| {
			error!("Error launching web server: {}", e);
			e
		})?.next().unwrap();
	match warp::serve(routes)
		.try_bind_with_graceful_shutdown(sock_tuple, async move {
			shut_rx.recv().await.expect("Terminated badly");
		}) {
		Ok((_, server)) => server.await,
		Err(e) => {
			error!("Unable to start: {}", e);
			return Err(e.into());
		}
	}
	info!("Web API shut down");
	Ok(())
}
