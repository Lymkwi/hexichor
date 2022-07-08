//! `Hexichor`, a URL status fetcher

// Nasty clippy lints
// Checks for garbage in the Cargo TOML files
#![deny(clippy::cargo)]
// Checks for needlessly complex structures
#![deny(clippy::complexity)]
// Checks for common invalid usage and workarounds
#![deny(clippy::correctness)]
// Checks for things that are typically forgotten by learners
#![deny(clippy::nursery)]
// Checks for mildly annoying comments it could make about your code
#![deny(clippy::pedantic)]
// Checks for inefficient ways to perform common tasks
#![deny(clippy::perf)]
// Checks for inefficient styling of code
#![deny(clippy::style)]
// Checks for potentially malicious behaviour
#![deny(clippy::suspicious)]

// Add some new clippy lints
// Checks for the use of a struct's name in its `impl`
#![deny(clippy::use_self)]
// Add some default lints
// Checks for unused variables
#![deny(unused_variables)]
// Deny missing documentation
#![deny(missing_docs)]
#![deny(rustdoc::missing_crate_level_docs)]
// Allow things I can't fix
#![allow(clippy::multiple_crate_versions)]
// The "const fn" lint is garbage rn and does not
// handle destructors well at all
#![allow(clippy::missing_const_for_fn)]

use clap::{
	Arg,
	Command
};
use tracing::{
	info,
	debug,
	warn,
	error
};
use tokio::{
	sync::{
		broadcast,
		mpsc
	}
};

use std::io;

mod api;
mod auth;
mod dto;
mod errors;
mod manager;
mod messages;

fn create_subscriber() -> Result<(), Box<dyn std::error::Error>> {
	let subscriber = tracing_subscriber::fmt()
		.compact()
		.with_file(true)
		.with_line_number(true)
		.with_thread_ids(true)
		.with_thread_names(false)
		.with_level(true)
		.with_target(false)
		.finish();
	tracing::subscriber::set_global_default(subscriber)?;
	Ok(())
}

#[tokio::main]
#[tracing::instrument(level="info")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args = Command::new("Hexichor")
		.version("0.0.1")
		.author("Lux A. Phifollen <contact@vulpinecitrus.info>")
		.about("A simple web API to let someone else handle fetching and reporting the status of batches of URLs")
		.subcommand_required(true)
		.subcommand(
			Command::new("mkpass")
				.about("Create a valid Argon2 hash for use in the credentials system")
				.arg(Arg::new("password")
					.short('p')
					.long("password")
					.value_name("password")
					.help("Optionally provide password in command line (note that this is insecure as it could be visible from your command line history)")
					.required(false)
					.takes_value(true)))
		.subcommand(
			Command::new("run")
				.about("Run the server")
				.arg(Arg::new("bind")
					.short('b')
					.long("bind")
					.env("HEXICHOR_HOST")
					.default_value("0.0.0.0")
					.takes_value(true)
					.help("Address to bind to"))
				.arg(Arg::new("port")
					.short('p')
					.long("port")
					.env("HEXICHOR_PORT")
					.value_name("port")
					.takes_value(true)
					.help("Port to bind to")))
		.get_matches();

	match args.subcommand() {
		Some(("run", cmd)) => {
			let host_bind: String = cmd.value_of("bind")
				.unwrap().into();
			let port_bind: u16 = cmd.value_of("port")
				.unwrap().parse()
				.map_err(|e: std::num::ParseIntError| {
					io::Error::new(
						io::ErrorKind::InvalidInput,
						e.to_string()
					)
				})?;
			run_server(host_bind, port_bind).await
		},
		Some(("mkpass", _cmd)) => {
			Ok(())
		},
		_ => {
			eprintln!("No subcommand provided");
			Err(
				io::Error::new(
					io::ErrorKind::NotFound,
					"Missing subcommand"
				).into()
			)
		}
	}
}


async fn run_server(host_bind: String, port_bind: u16) -> Result<(), Box<dyn std::error::Error>> {
	create_subscriber()?;
	debug!("Logger initialized");

	// Some queues we need
	let (req_tx, req_rx) = mpsc::channel(64);
	let (poll_tx, poll_rx) = mpsc::channel(64);
	let (shut_tx, shut_rx) = broadcast::channel(1);

	// Manager thread
	let manager_handle = tokio::spawn(async move {
		manager::manager(
			req_rx,
			poll_rx,
			shut_rx
		).await
	});

	// Use the broadcast channel to get shut down by API on failure
	let mut shut_ourselves = shut_tx.subscribe();

	// Web API
	let shut_rx = shut_tx.subscribe();
	let shut_inx = shut_tx.clone();
	let api_handle = tokio::spawn(async move {
		if api::start_api(
				(&host_bind, port_bind),
				req_tx,
				poll_tx,
				shut_rx
				).await.is_err() {
			error!("Signaling shutdown");
			shut_inx.send(())
				.expect("API could not start, but we are unable to signal for shutdown. Panicking.");
		}
	});

	tokio::select! {
		a = tokio::signal::ctrl_c() => {
			if let Err(err) = a {
				error!("Unable to listen to ^C signal: {}", err);
			}
		}
		_ = shut_ourselves.recv() => {
			// Shutting down
		}
		else => {
			info!("Unhandled");
		}
	}
	info!("Shutting down");

	// Shut down
	std::mem::drop(shut_tx.send(()));
	std::mem::drop(api_handle.await);
	info!("Web API terminated");

	// Shut down the manager
	std::mem::drop(manager_handle.await);
	info!("Manager terminated");

	Ok(())
}
