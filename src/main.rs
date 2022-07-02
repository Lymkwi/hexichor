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


use tracing::{
    info,
    debug,
    warn,
    error
};
use tokio::{
    sync::{
        mpsc
    }
};

mod api;
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
    create_subscriber()?;
    debug!("Logger initialized");

    // Some queues we need
    let (req_tx, req_rx) = mpsc::channel(64);
    let (poll_tx, poll_rx) = mpsc::channel(64);

    // Manager thread
    let manager_handle = tokio::spawn(async move {
        manager::manager(
            req_rx,
            poll_rx
        ).await
    });

    // Web API
    let api_handle = tokio::spawn(async move {
        api::start_api(
            req_tx,
            poll_tx
        ).await
    });

    match tokio::signal::ctrl_c().await {
        Ok(()) => {},
        Err(err) => {
            error!("Unable to listen to ^C signal: {}", err);
        }
    }
    info!("Shutting down");

    // Shut down the API first
    api_handle.abort();
    std::mem::drop(api_handle.await);
    info!("Web API terminated");

    // Shut down the manager
    manager_handle.abort();
    std::mem::drop(manager_handle.await);
    info!("Manager terminated");

    Ok(())
}
