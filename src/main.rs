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
use crate::draft_engine::{DraftItemId, GameState, PlayerId};
use crate::lobby_manager::{DraftLobbyId, LobbyManagerResponse, LobbyStateForPlayer};


fn make_new_draft_response(lobby_id: DraftLobbyId) -> warp::reply::Response {
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("template", "www/share_game_template.html").unwrap();

    let mut data = serde_json::Map::new();
    let game_url = format!("http://localhost:3030/join_draft/{}", lobby_id);
    data.insert("share_url".to_string(), handlebars::to_json(game_url));

    let render = handlebars.render("template", &data).unwrap();
    warp::reply::html(render).into_response()
}

fn make_redirect_to_game_response(lobby_id: DraftLobbyId, player_id: PlayerId) -> warp::reply::Response {
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

async fn join_draft_page(lobby_id: DraftLobbyId) -> Result<impl warp::Reply, std::convert::Infallible> {
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("template", "www/join_game_template.html").unwrap();

    let mut data = serde_json::Map::new();
    let url = format!("http://localhost:3030/join_draft/{}", lobby_id);
    data.insert("url_to_submit".to_string(), handlebars::to_json(url));

    let render = handlebars.render("template", &data).unwrap();
    Ok(warp::reply::html(render))
}

async fn post_playername(mpsc_tx: tokio::sync::mpsc::Sender<lobby_manager::LobbyManagerTask>, lobby_id: DraftLobbyId, simple_map: HashMap<String, String>) -> Result<warp::reply::Response, std::convert::Infallible> {
    let player_name = simple_map.get("player_name").cloned();
    if player_name.is_none() {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }
    let player_name = player_name.unwrap();
    if player_name.is_empty() || !player_name.is_ascii() || player_name.len() > 20 {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = lobby_manager::LobbyManagerTask {
        request: lobby_manager::LobbyManagerRequest::JoinLobby {
            lobby_id,
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
            LobbyManagerResponse::LobbyJoined { lobby_id, player_id } => Ok(make_redirect_to_game_response(lobby_id, player_id)),
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

async fn get_draft_page(mpsc_tx: tokio::sync::mpsc::Sender<lobby_manager::LobbyManagerTask>, lobby_id: DraftLobbyId, player_id: PlayerId) -> Result<warp::reply::Response, std::convert::Infallible> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = lobby_manager::LobbyManagerTask {
        request: lobby_manager::LobbyManagerRequest::GetLobbyState {lobby_id, player_id},
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

    let lobby_state: LobbyStateForPlayer = match f.await {
        Ok(response) => match response {
            LobbyManagerResponse::LobbyErrorMsg(e) => {
                log::warn!("Returning LobbyErrorMsg {e} to end-client");
                return Ok(warp::reply::html(e.to_string()).into_response());
            }
            LobbyManagerResponse::LobbyState(state) => state,
            _ => {
                log::error!("Unexpected task response for GetLobbyState");
                return Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        },
        Err(e) => {
            log::error!("Didn't receive task response: {e}");
            return Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };

    log::debug!("{lobby_state:?}");

    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("template", "www/draft_template.html").unwrap();

    let mut data = serde_json::Map::new();

    // hack to get this templating right
    let mut pickable_items: Vec<HashMap<String, String>> = vec![];
    for (draft_item_id, template) in lobby_state.pending_picks {
        let mut temp_map: HashMap<String, String> = HashMap::new();
        temp_map.insert("pokepaste".to_string(), template);
        temp_map.insert("draft_id".to_string(), draft_item_id.to_string());
        pickable_items.push(temp_map)
    }

    let waiting_for_pack: bool = !lobby_state.draft_is_finished && pickable_items.is_empty() && !lobby_state.allocated_picks.is_empty();

    data.insert("lobby_id".to_string(), handlebars::to_json(&lobby_state.lobby_id));
    data.insert("player_id".to_string(), handlebars::to_json(&lobby_state.player_id));
    data.insert("joining_players".to_string(), handlebars::to_json(&lobby_state.joining_players));
    data.insert("open_slots".to_string(), handlebars::to_json(&lobby_state.open_slots));
    data.insert("pending_picks".to_string(), handlebars::to_json(&pickable_items));
    data.insert("allocated_picks".to_string(), handlebars::to_json(&lobby_state.allocated_picks));
    data.insert("game_state".to_string(), handlebars::to_json(&lobby_state.game_state));
    data.insert("waiting_for_pack".to_string(), handlebars::to_json(waiting_for_pack));
    data.insert("time_left_s".to_string(), handlebars::to_json(&lobby_state.time_to_pick_s));
    // todo: url needs to be draft/lobby_id/player_id (or just template it)
    data.insert("draft_order".to_string(), handlebars::to_json(&lobby_state.draft_order));
    data.insert("draft_is_finished".to_string(), handlebars::to_json(&lobby_state.draft_is_finished));
    let target_url = format!("http://localhost:3030/draft/{lobby_id}/{player_id}");
    data.insert("url_for_draft".to_string(), handlebars::to_json(&target_url));


    let render = handlebars.render("template", &data).unwrap();
    Ok(warp::reply::html(render).into_response())
}

async fn handle_draft_post(mpsc_tx: tokio::sync::mpsc::Sender<lobby_manager::LobbyManagerTask>, lobby_id: DraftLobbyId, player_id: PlayerId, post_data: DraftPost) -> Result<impl warp::Reply, std::convert::Infallible> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = match post_data.command.as_str() {
        "start_game" => lobby_manager::LobbyManagerRequest::StartLobby { lobby_id },
        "pick" => lobby_manager::LobbyManagerRequest::MakePick { lobby_id, player_id, pick: post_data.pick_id },
        "poll" => lobby_manager::LobbyManagerRequest::BlockForUpdate {lobby_id, player_id, game_state: post_data.game_state},
        _ => return Ok(StatusCode::BAD_REQUEST.into_response()),
    };
    let task = lobby_manager::LobbyManagerTask { request, response_channel: tx };

    match mpsc_tx.send(task).await {
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
            LobbyManagerResponse::LobbyStarted => Ok(StatusCode::OK.into_response()),
            LobbyManagerResponse::PickMade => Ok(StatusCode::OK.into_response()),
            LobbyManagerResponse::UpdateReady => Ok(StatusCode::OK.into_response()),
            _ => {
                log::error!("Unexpected task response");
                Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
            }
        },
        Err(e) => {
            log::error!("Didn't receive task response: {e}");
            Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
struct DraftPost {
    command: String,
    lobby_id: DraftLobbyId,
    player_id: PlayerId,
    pick_id: DraftItemId,
    game_state: GameState,
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

    let temp_clone = mpsc_tx.clone();
    let new_draft_tx = warp::any().map(move || temp_clone.clone());

    let temp_clone = mpsc_tx.clone();
    let join_draft_tx = warp::any().map(move || temp_clone.clone());

    let temp_clone = mpsc_tx.clone();
    let draft_route_tx = warp::any().map(move || temp_clone.clone());

    let temp_clone = mpsc_tx.clone();
    let draft_post_tx = warp::any().map(move || temp_clone.clone());


    let index_route = warp::path::end().and(warp::fs::file("www/static/index.html"));
    let static_route = warp::path("static").and(warp::fs::dir("www/static"));
    let draft_route = warp::get()
        .and(draft_route_tx)
        .and(warp::path!("draft" / DraftLobbyId / PlayerId))
        .and_then(get_draft_page);
    let draft_route_post = warp::post()
        .and(draft_post_tx)
        .and(warp::path!("draft" / DraftLobbyId / PlayerId))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(handle_draft_post);

    let create_draft_route = warp::get()
        .and(new_draft_tx)
        .and(warp::path("new_draft"))
        .and_then(new_draft);
    let join_draft_get_route = warp::get().and(warp::path!("join_draft" / DraftLobbyId))
        .and_then(join_draft_page);
    let join_draft_post_route = warp::post()
        .and(join_draft_tx)
        .and(warp::path!("join_draft" / DraftLobbyId))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::form())
        .and_then(post_playername);


    let routes = warp::get()
        .and(index_route)
        .or(static_route)
        .or(create_draft_route)
        .or(draft_route)
        .or(draft_route_post)
        .or(join_draft_get_route)
        .or(join_draft_post_route);

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
