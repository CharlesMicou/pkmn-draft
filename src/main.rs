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
        },
        Err(_) => {
            log::info!("Starting server on http://localhost:3030");
            SocketAddr::from(([127, 0, 0, 1], 3030))
        }
    };


    let database = draft_database::DraftDatabase::from_folder("data").unwrap();

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

    let webserver = routes::make_server(configured_addr, mpsc_tx, shutdown_rx);
    tokio::task::spawn(webserver);

    log::info!("Server Ready");

    match lobby_manager_thread.join() {
        Ok(_) => log::warn!("Closing server"),
        Err(_) => log::error!("Lobby manager did not exit gracefully")
    }
}
