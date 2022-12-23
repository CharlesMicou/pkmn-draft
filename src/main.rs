mod lobby_manager;
mod draft_engine;
mod draft_database;

use warp::{Filter, Reply};
use warp::http::StatusCode;
use handlebars;
use serde::Serialize;
use serde_json::json;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use simple_logger::SimpleLogger;
use log::{info, warn, error};
use std::future::IntoFuture;
use crate::lobby_manager::LobbyManagerResponse;


fn make_new_draft_response(lobby_id: u64) -> warp::reply::Response {
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("template", "www/share_game_template.html").unwrap();

    let mut data = serde_json::Map::new();
    let game_url = format!("http://localhost:3030/join_draft/{}", lobby_id);
    data.insert("share_url".to_string(), handlebars::to_json(game_url));

    let render = handlebars.render("template", &data).unwrap();
    warp::reply::html(render).into_response()
}

fn make_redirect_to_game_response(lobby_id: u64, player_id: u64) -> warp::reply::Response {
    let body = format!(r#"
<html>
    <meta http-equiv="Refresh" content="0; url='http://localhost:3030/draft/{lobby_id}/{player_id}'" />
</html>
"#);
    warp::reply::html(body).into_response()
}

async fn new_draft(mpsc_tx: tokio::sync::mpsc::Sender<lobby_manager::LobbyManagerTask>) -> Result<warp::reply::Response, std::convert::Infallible> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = lobby_manager::LobbyManagerTask {
        request: lobby_manager::LobbyManagerRequest::CreateLobby,
        response_channel: tx,
    };

    match mpsc_tx.send(request).await {
        Ok(_) => (),
        Err(e) => {
            log::error!("Failed to enqueue task: {e}");
            return Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };

    let f = rx.into_future();

    match f.await {
        Ok(response) => match response {
            LobbyManagerResponse::LobbyErrorMsg(e) => {
                log::warn!("Returning LobbyErrorMsg {e} to end-client");
                Ok(warp::reply::html(e.to_string()).into_response())
            }
            LobbyManagerResponse::LobbyCreated(id) => Ok(make_new_draft_response(id)),
            _ => {
                log::error!("Unexpected task response for CreateLobby");
                Ok(warp::reply::html("foo").into_response())
            }
        },
        Err(e) => {
            log::error!("Didn't receive task response: {e}");
            Ok(warp::reply::html("foo").into_response())
        }
    }
}

async fn join_draft_page(game_id: u64) -> Result<impl warp::Reply, std::convert::Infallible> {
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("template", "www/join_game_template.html").unwrap();

    let mut data = serde_json::Map::new();
    let url = format!("http://localhost:3030/join_draft/{}", game_id);
    data.insert("url_to_submit".to_string(), handlebars::to_json(url));

    let render = handlebars.render("template", &data).unwrap();
    Ok(warp::reply::html(render))
}

async fn post_playername(mpsc_tx: tokio::sync::mpsc::Sender<lobby_manager::LobbyManagerTask>, game_id: u64, simple_map: HashMap<String, String>) -> Result<warp::reply::Response, std::convert::Infallible> {
    let player_name = simple_map.get("player_name").cloned();
    if player_name.is_none() {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }
    let player_name = player_name.unwrap();

    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = lobby_manager::LobbyManagerTask {
        request: lobby_manager::LobbyManagerRequest::JoinLobby {
            lobby_id: game_id,
            player_name,
        },
        response_channel: tx,
    };

    match mpsc_tx.send(request).await {
        Ok(_) => (),
        Err(e) => {
            log::error!("Failed to enqueue task: {e}");
            return Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };

    let f = rx.into_future();
    match f.await {
        Ok(response) => match response {
            LobbyManagerResponse::LobbyErrorMsg(e) => {
                log::warn!("Returning LobbyErrorMsg {e} to end-client");
                Ok(warp::reply::html(e.to_string()).into_response())
            }
            LobbyManagerResponse::LobbyJoined{lobby_id, player_id} => Ok(make_redirect_to_game_response(lobby_id, player_id)),
            _ => {
                log::error!("Unexpected task response for JoinLobby");
                Ok(warp::reply::html("foo").into_response())
            }
        },
        Err(e) => {
            log::error!("Didn't receive task response: {e}");
            Ok(warp::reply::html("foo").into_response())
        }
    }
}

async fn draft_page() -> Result<impl warp::Reply, std::convert::Infallible> {
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("template", "www/draft_template.html").unwrap();

    let mut data = serde_json::Map::new();
    data.insert("lobby_id".to_string(), handlebars::to_json(123));
    data.insert("player_id".to_string(), handlebars::to_json(456));
    data.insert("url_for_draft".to_string(), handlebars::to_json("http://localhost:3030/test_post"));

    let render = handlebars.render("template", &data).unwrap();
    Ok(warp::reply::html(render))
}

#[derive(Deserialize, Serialize, Debug)]
struct DummyPost {
    lobby_id: u64,
    player_id: u64,
    pick_id: u64,
}


#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_colors(true)
        .with_utc_timestamps()
        .env().init()

        .unwrap();
    log::info!("Starting server on http://localhost:3030");
    let database = draft_database::DraftDatabase::from_folder("data/generated").unwrap();


    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let (mpsc_tx, mut mpsc_rx): (tokio::sync::mpsc::Sender<lobby_manager::LobbyManagerTask>, tokio::sync::mpsc::Receiver<lobby_manager::LobbyManagerTask>) = tokio::sync::mpsc::channel(1);
    let mut lobby_manager = lobby_manager::LobbyManager::new(mpsc_rx, database);

    let lobby_manager_thread = std::thread::spawn(move || {
        lobby_manager.run();
        match shutdown_tx.send(()) {
            Ok(_) => (),
            Err(_) => log::error!("Failed to send shutdown signal"),
        }
    });

    let clone_a = mpsc_tx.clone();
    let mspc_tx_a = warp::any().map(move || clone_a.clone());

    let clone_b = mpsc_tx.clone();
    let mspc_tx_b = warp::any().map(move || clone_b.clone());


    let index_route = warp::path::end().and(warp::fs::file("www/static/index.html"));
    let static_route = warp::path("static").and(warp::fs::dir("www/static"));
    let draft_route = warp::path("draft")
        .and_then(draft_page);

    let new_draft_route = warp::get()
        .and(mspc_tx_a)
        .and(warp::path("new_draft"))
        .and_then(new_draft);
    let join_draft_get_route = warp::get().and(warp::path!("join_draft" / u64))
        .and_then(join_draft_page);
    let join_draft_post_route = warp::post()
        .and(mspc_tx_b)
        .and(warp::path!("join_draft" / u64))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::form())
        .and_then(post_playername);


    let post_route = warp::post()
        .and(warp::path("test_post"))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .map(|mut posted: DummyPost| {
            log::info!("{:?}", posted);
            warp::reply::json(&posted)
        });

    let routes = warp::get()
        .and(index_route)
        .or(static_route)
        .or(new_draft_route)
        .or(draft_route)
        .or(join_draft_get_route)
        .or(join_draft_post_route)
        .or(post_route);

    let (_addr, warp_server) = warp::serve(routes).bind_with_graceful_shutdown(([127, 0, 0, 1], 3030), async {
        shutdown_rx.await.ok();
    });

    log::info!("Server Ready");

    tokio::task::spawn(warp_server);
    match lobby_manager_thread.join() {
        Ok(_) => log::warn!("Closing server"),
        Err(_) => log::error!("Lobby manager did not exit gracefully")
    }
}
