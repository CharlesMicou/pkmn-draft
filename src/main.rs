mod lobby_manager;
mod draft_engine;

use warp::Filter;

async fn new_lobby() -> Result<impl warp::Reply, std::convert::Infallible> {
    Ok(warp::http::Response::builder().body("Foo"))
}

#[tokio::main]
async fn main() {
    println!("Starting up");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let (mpsc_tx, mut mpsc_rx): (tokio::sync::mpsc::Sender<lobby_manager::LobbyManagerTask>, tokio::sync::mpsc::Receiver<lobby_manager::LobbyManagerTask>) = tokio::sync::mpsc::channel(1);
    let mut lobby_manager = lobby_manager::LobbyManager::new(mpsc_rx);

    let lobby_manager_thread = std::thread::spawn(move || {
        lobby_manager.run();
        match shutdown_tx.send(()) {
            Ok(_) => (),
            Err(_) => println!("Failed to send shutdown signal"),
        }
    });

    let _mpsc_tx = warp::any().map(move || mpsc_tx.clone());


    let static_route = warp::path::end().and(warp::fs::dir("www/static"));

    // let new_lobby_route = warp::path("new_lobby").and(new_lobby);

    let (_addr, warp_server) = warp::serve(static_route).bind_with_graceful_shutdown(([127, 0, 0, 1], 3030), async {
        shutdown_rx.await.ok();
    });

    println!("Server Ready");

    tokio::task::spawn(warp_server);
    match lobby_manager_thread.join() {
        Ok(_) => println!("Closing server"),
        Err(_) => println!("Lobby manager did not exit gracefully")
    }
}
