use std::collections::HashMap;
use std::io;
use crate::draft_engine;
use crate::draft_engine::{DraftItemId, DraftLobby, PlayerId};
use crate::draft_database::DraftDatabase;
use log::{info, warn, error};


pub type DraftLobbyId = u64;
pub type LobbyState = i32; // todo

pub enum LobbyManagerRequest {
    CreateLobby,
    JoinLobby { lobby_id: DraftLobbyId, player_name: String },
    StartLobby { lobby_id: DraftLobbyId},
    PollLobby { lobby_id: DraftLobbyId, player_id: PlayerId, game_state: u64 },
    MakePick { lobby_id: DraftLobbyId, player_id: PlayerId, pick: DraftItemId },
}

pub enum LobbyManagerResponse {
    LobbyErrorMsg(String),
    LobbyCreated(DraftLobbyId),
    LobbyJoined { lobby_id: DraftLobbyId, player_id: PlayerId },
    LobbyStarted,
    LobbyState(LobbyState),
}

pub struct LobbyManagerTask {
    pub request: LobbyManagerRequest,
    pub response_channel: tokio::sync::oneshot::Sender<LobbyManagerResponse>,
}

const MAX_LOBBY_CAPACITY: usize = 6;

pub struct LobbyManager {
    draft_database: DraftDatabase,
    active_lobbies: HashMap<DraftLobbyId, draft_engine::DraftLobby>,
    next_lobby_id: DraftLobbyId,
    task_queue: tokio::sync::mpsc::Receiver<LobbyManagerTask>,
}

impl LobbyManager {
    pub fn new(request_queue: tokio::sync::mpsc::Receiver<LobbyManagerTask>, draft_database: DraftDatabase) -> LobbyManager {
        LobbyManager {
            draft_database,
            active_lobbies: HashMap::new(),
            next_lobby_id: 0,
            task_queue: request_queue,
        }
    }

    pub fn run(&mut self) -> () {
        while let Some(task) = self.task_queue.blocking_recv() {
            match task.response_channel.send(self.process_request(task.request)) {
                Ok(_) => {}
                Err(_) => log::warn!("Could not respond to request, as receiver dropped."),
            }
        }
    }

    fn process_request(&mut self, request: LobbyManagerRequest) -> LobbyManagerResponse {
        let f = "Not implemented";
        match request {
            LobbyManagerRequest::CreateLobby => LobbyManagerResponse::LobbyCreated(self.create_lobby()),
            LobbyManagerRequest::JoinLobby{lobby_id, player_name} => {
                let player_name_copy_for_logging = player_name.clone();
                match self.active_lobbies.get_mut(&lobby_id) {
                    Some(lobby) => match lobby.add_player(player_name) {
                        Ok(player_id) => {
                            log::info!("Added {player_name_copy_for_logging} to lobby {lobby_id} with player_id {player_id}");
                            LobbyManagerResponse::LobbyJoined {
                                lobby_id,
                                player_id,
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to add {player_name_copy_for_logging} to lobby {lobby_id}: {e}");
                            LobbyManagerResponse::LobbyErrorMsg(e.to_string())
                        }
                    },
                    None => {
                        log::warn!("Player name {player_name} submitted to unknown lobby {lobby_id}");
                        LobbyManagerResponse::LobbyErrorMsg("Lobby not found".to_string())
                    }
                }
            },
            LobbyManagerRequest::StartLobby {lobby_id} => {
                match self.start_draft(lobby_id) {
                    Ok(_) => {
                        LobbyManagerResponse::LobbyStarted
                    }
                    Err(e) => {
                        log::warn!("Failed to start lobby {lobby_id}: {e}");
                        LobbyManagerResponse::LobbyErrorMsg("Lobby did not start".to_string())
                    }
                }
            }
            _ => LobbyManagerResponse::LobbyErrorMsg("Not implemented".to_string())
        }
    }

    pub fn create_lobby(&mut self) -> DraftLobbyId {
        let lobby_id = self.next_lobby_id;
        log::info!("Creating lobby {lobby_id}");
        self.active_lobbies.insert(lobby_id, draft_engine::DraftLobby::new(MAX_LOBBY_CAPACITY));
        self.next_lobby_id += 1;
        lobby_id
    }

    pub fn get_lobby(&self, lobby_id: DraftLobbyId) -> Option<&draft_engine::DraftLobby> {
        self.active_lobbies.get(&lobby_id)
    }

    pub fn start_draft(&mut self, lobby_id: DraftLobbyId) -> io::Result<()> {
        match self.active_lobbies.get_mut(&lobby_id) {
            Some(lobby) => lobby.start(self.draft_database.get_item_list()),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Lobby not found"))
        }
    }
}