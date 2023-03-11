use std::env;
use std::net::SocketAddr;

use simple_logger::SimpleLogger;

use crate::lobby_manager::{LobbyManagerResponse};

mod lobby_manager;
mod draft_engine;
mod draft_database;
mod routes;


#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_colors(true)
        .with_utc_timestamps()
        .env().init()
        .unwrap();

    let configured_addr: SocketAddr = match env::var("PKMNDRAFT_PORT") {
        Ok(val) => {
            let parsed_port: u16 = val.trim().parse().unwrap();
            log::info!("Starting server on port {parsed_port}");
            SocketAddr::from(([0, 0, 0, 0], parsed_port))
        }
        Err(_) => {
            log::info!("Starting server on http://localhost:3030");
            SocketAddr::from(([127, 0, 0, 1], 3030))
        }
    };

    let cert_path: Option<String> = match env::var("HTTPS_CERT") {
        Ok(val) => Some(val),
        Err(_) => None
    };
    let key_path: Option<String> = match env::var("HTTPS_KEY") {
        Ok(val) => Some(val),
        Err(_) => None
    };

    let https_credentials = if cert_path.is_some() && key_path.is_some() {
        log::info!("Found HTTPS credentials.");
        Some((cert_path.unwrap(), key_path.unwrap()))
    } else {
        None
    };


    // let database = draft_database::DraftSet::from_folder("data/all_stars").unwrap();
    let database = draft_database::DraftDb::from_folder("data").unwrap();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let (mpsc_tx, mpsc_rx): (tokio::sync::mpsc::Sender<lobby_manager::LobbyManagerTask>, tokio::sync::mpsc::Receiver<lobby_manager::LobbyManagerTask>) = tokio::sync::mpsc::channel(1);
    let lobby_manager_self_queue = mpsc_tx.clone();
    let mut lobby_manager = lobby_manager::LobbyManager::new(mpsc_rx, lobby_manager_self_queue, database);

    let lobby_manager_thread = std::thread::spawn(move || {
        lobby_manager.run();
        match shutdown_tx.send(()) {
            Ok(_) => (),
            Err(_) => log::error!("Failed to send shutdown signal"),
        }
    });

    match https_credentials {
        Some(credentials) => {
            let webserver = routes::make_server_with_tls(configured_addr, credentials, mpsc_tx, shutdown_rx);
            tokio::task::spawn(webserver);
            let redirect_server = routes::make_https_redirect_server();
            tokio::task::spawn(redirect_server);
        },
        None => {
            let webserver = routes::make_server(configured_addr, mpsc_tx, shutdown_rx);
            tokio::task::spawn(webserver);
        }
    };

    log::info!("Server Ready");

    match lobby_manager_thread.join() {
        Ok(_) => log::warn!("Closing server"),
        Err(_) => log::error!("Lobby manager did not exit gracefully")
    }
}
