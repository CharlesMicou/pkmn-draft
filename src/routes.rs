use warp::{Filter, Reply};
use warp::http::{StatusCode, Uri};
use std::collections::HashMap;
use std::future::IntoFuture;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use handlebars;
use serde_derive::{Deserialize, Serialize};

use crate::draft_engine::{DraftItemId, GameState, PlayerId};
use crate::lobby_manager::{DraftLobbyId, LobbyManagerRequest, LobbyManagerResponse, LobbyStateForPlayer, LobbyManagerTask};

pub fn make_server_with_tls(configured_addr: SocketAddr,
                            https_paths: (String, String),
                            lobby_manager_task_queue: tokio::sync::mpsc::Sender<LobbyManagerTask>,
                            shutdown_signal: tokio::sync::oneshot::Receiver<()>) -> impl Future<Output=()> {
    let mspc_tx = warp::any().map(move || lobby_manager_task_queue.clone());
    let mut handlebars: handlebars::Handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("draft_template", "www/draft_template.html").unwrap();
    handlebars.register_template_file("share_game_template", "www/share_game_template.html").unwrap();
    let handlebars = Arc::new(handlebars);
    let handlebars = warp::any().map(move || handlebars.clone());

    let index_route = warp::path::end().and(warp::fs::file("www/static/index.html"));
    let static_route = warp::path("static").and(warp::fs::dir("www/static"));
    let draft_route = warp::get()
        .and(mspc_tx.clone())
        .and(handlebars.clone())
        .and(warp::path!("draft" / DraftLobbyId / PlayerId))
        .and_then(get_draft_page);
    let draft_route_post = warp::post()
        .and(mspc_tx.clone())
        .and(warp::path!("draft" / DraftLobbyId / PlayerId))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(handle_draft_post);

    let create_draft_route = warp::get()
        .and(mspc_tx.clone())
        .and(handlebars.clone())
        .and(warp::path!("new_draft" / String ))
        .and_then(new_draft);
    let join_draft_get_route = warp::get()
        .and(warp::path("join_draft"))
        .and(warp::fs::file("www/join_game_template.html"));
    let join_draft_post_route = warp::post()
        .and(mspc_tx.clone())
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

    let (cert, key) = https_paths;
    let (_addr, warp_server) = warp::serve(routes)
        .tls()
        .cert_path(cert)
        .key_path(key)
        .bind_with_graceful_shutdown(configured_addr, async {
            shutdown_signal.await.ok();
        });
    warp_server
}

pub fn make_https_redirect_server() -> impl Future<Output=()> {
    let http_route = warp::any()
        .map(|| warp::redirect(Uri::from_static("https://happylittleneurons.com")));
    warp::serve(http_route).bind(([0, 0, 0, 0], 80))
}

pub fn make_server(configured_addr: SocketAddr,
                   lobby_manager_task_queue: tokio::sync::mpsc::Sender<LobbyManagerTask>,
                   shutdown_signal: tokio::sync::oneshot::Receiver<()>) -> impl Future<Output=()> {
    let mspc_tx = warp::any().map(move || lobby_manager_task_queue.clone());
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("draft_template", "www/draft_template.html").unwrap();
    handlebars.register_template_file("share_game_template", "www/share_game_template.html").unwrap();
    let handlebars = Arc::new(handlebars);
    let handlebars = warp::any().map(move || handlebars.clone());

    let index_route = warp::path::end().and(warp::fs::file("www/static/index.html"));
    let static_route = warp::path("static").and(warp::fs::dir("www/static"));
    let draft_route = warp::get()
        .and(mspc_tx.clone())
        .and(handlebars.clone())
        .and(warp::path!("draft" / DraftLobbyId / PlayerId))
        .and_then(get_draft_page);
    let draft_route_post = warp::post()
        .and(mspc_tx.clone())
        .and(warp::path!("draft" / DraftLobbyId / PlayerId))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(handle_draft_post);

    let create_draft_route = warp::get()
        .and(mspc_tx.clone())
        .and(handlebars.clone())
        .and(warp::path!("new_draft" / String ))
        .and_then(new_draft);
    let join_draft_get_route = warp::get()
        .and(warp::path("join_draft"))
        .and(warp::fs::file("www/join_game_template.html"));
    let join_draft_post_route = warp::post()
        .and(mspc_tx.clone())
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


    let (_addr, warp_server) = warp::serve(routes)
        .bind_with_graceful_shutdown(configured_addr, async {
            shutdown_signal.await.ok();
        });
    warp_server
}

fn make_new_draft_response(handlebars: Arc<handlebars::Handlebars<'_>>, lobby_id: DraftLobbyId) -> warp::reply::Response {
    let mut data = serde_json::Map::new();
    data.insert("lobby_id".to_string(), handlebars::to_json(lobby_id));

    let render = handlebars.render("share_game_template", &data).unwrap();
    warp::reply::html(render).into_response()
}

fn make_redirect_to_game_response(lobby_id: DraftLobbyId, player_id: PlayerId) -> warp::reply::Response {
    let body = format!(r#"
<html>
    <meta http-equiv="Refresh" content="0; url='/draft/{lobby_id}/{player_id}'" />
</html>
"#);
    warp::reply::html(body).into_response()
}

async fn new_draft(mpsc_tx: tokio::sync::mpsc::Sender<LobbyManagerTask>, handlebars: Arc<handlebars::Handlebars<'_>>, set_name: String) -> Result<warp::reply::Response, std::convert::Infallible> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = LobbyManagerTask {
        request: LobbyManagerRequest::CreateLobby{set_name},
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
            LobbyManagerResponse::LobbyCreated(id) => Ok(make_new_draft_response(handlebars, id)),
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

async fn post_playername(mpsc_tx: tokio::sync::mpsc::Sender<LobbyManagerTask>, lobby_id: DraftLobbyId, simple_map: HashMap<String, String>) -> Result<warp::reply::Response, std::convert::Infallible> {
    let player_name = simple_map.get("player_name").cloned();
    if player_name.is_none() {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }
    let player_name = player_name.unwrap();
    if player_name.is_empty() || !player_name.is_ascii() || player_name.len() > 20 {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = LobbyManagerTask {
        request: LobbyManagerRequest::JoinLobby {
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

async fn get_draft_page(mpsc_tx: tokio::sync::mpsc::Sender<LobbyManagerTask>, handlebars: Arc<handlebars::Handlebars<'_>>, lobby_id: DraftLobbyId, player_id: PlayerId) -> Result<warp::reply::Response, std::convert::Infallible> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = LobbyManagerTask {
        request: LobbyManagerRequest::GetLobbyState { lobby_id, player_id },
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

    let mut data = serde_json::Map::new();

    // hack to get this templating right
    let mut pickable_items: Vec<HashMap<String, String>> = vec![];
    for (draft_item_id, template, stats) in lobby_state.pending_picks {
        let mut temp_map: HashMap<String, String> = HashMap::new();
        temp_map.insert("pokepaste".to_string(), template);
        temp_map.insert("pokestats".to_string(), stats);
        temp_map.insert("draft_id".to_string(), draft_item_id.to_string());
        pickable_items.push(temp_map)
    }

    let waiting_for_pack: bool = !lobby_state.draft_is_finished && pickable_items.is_empty() && !lobby_state.allocated_picks.is_empty();

    let mut allocated_items: Vec<HashMap<String, String>> = vec![];
    for (template, stats) in lobby_state.allocated_picks {
        let mut temp_map: HashMap<String, String> = HashMap::new();
        temp_map.insert("pokepaste".to_string(), template);
        temp_map.insert("pokestats".to_string(), stats);
        allocated_items.push(temp_map)
    }

    let (current_round, total_rounds, current_pick, pack_size) = &lobby_state.rounds_and_picks;

    data.insert("lobby_id".to_string(), handlebars::to_json(&lobby_state.lobby_id));
    data.insert("player_id".to_string(), handlebars::to_json(&lobby_state.player_id));
    data.insert("joining_players".to_string(), handlebars::to_json(&lobby_state.joining_players));
    data.insert("open_slots".to_string(), handlebars::to_json(&lobby_state.open_slots));
    data.insert("pending_picks".to_string(), handlebars::to_json(&pickable_items));
    data.insert("allocated_picks".to_string(), handlebars::to_json(&allocated_items));
    data.insert("game_state".to_string(), handlebars::to_json(&lobby_state.game_state));
    data.insert("waiting_for_pack".to_string(), handlebars::to_json(waiting_for_pack));
    data.insert("time_left_s".to_string(), handlebars::to_json(&lobby_state.time_to_pick_s));
    data.insert("draft_order".to_string(), handlebars::to_json(&lobby_state.draft_order));
    data.insert("draft_is_finished".to_string(), handlebars::to_json(&lobby_state.draft_is_finished));
    data.insert("current_round".to_string(), handlebars::to_json(current_round));
    data.insert("total_rounds".to_string(), handlebars::to_json(total_rounds));
    data.insert("current_pick".to_string(), handlebars::to_json(current_pick));
    data.insert("pack_size".to_string(), handlebars::to_json(pack_size));
    data.insert("raw_allocated_picks".to_string(), handlebars::to_json(&lobby_state.raw_picks));

    let render = handlebars.render("draft_template", &data).unwrap();
    Ok(warp::reply::html(render).into_response())
}

#[derive(Deserialize, Serialize, Debug)]
struct DraftPost {
    command: String,
    lobby_id: DraftLobbyId,
    player_id: PlayerId,
    pick_id: DraftItemId,
    game_state: GameState,
}

async fn handle_draft_post(mpsc_tx: tokio::sync::mpsc::Sender<LobbyManagerTask>, lobby_id: DraftLobbyId, player_id: PlayerId, post_data: DraftPost) -> Result<impl warp::Reply, std::convert::Infallible> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let request = match post_data.command.as_str() {
        "start_game" => LobbyManagerRequest::StartLobby { lobby_id },
        "pick" => LobbyManagerRequest::MakePick { lobby_id, player_id, pick: post_data.pick_id },
        "poll" => LobbyManagerRequest::BlockForUpdate { lobby_id, player_id, game_state: post_data.game_state },
        _ => return Ok(StatusCode::BAD_REQUEST.into_response()),
    };
    let task = LobbyManagerTask { request, response_channel: tx };

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
