use std::collections::HashMap;
use std::io;
use crate::draft_engine;
use crate::draft_engine::{DraftItemId, DraftLobby, PlayerId};
use crate::draft_database::DraftDatabase;
use log::{info, warn, error};
use crate::LobbyManagerResponse::LobbyState;


pub type DraftLobbyId = u64;

#[derive(Debug)]
pub struct LobbyStateForPlayer {
    pub lobby_id: DraftLobbyId,
    pub player_id: PlayerId,
    pub joining_players: Vec<String>,
    pub open_slots: Vec<String>,
    pub pending_picks: Vec<(DraftItemId, String)>,
    pub allocated_picks: Vec<String>,
    pub game_state: u64,
}

pub enum LobbyManagerRequest {
    CreateLobby,
    JoinLobby { lobby_id: DraftLobbyId, player_name: String },
    StartLobby { lobby_id: DraftLobbyId },
    GetLobbyState { lobby_id: DraftLobbyId, player_id: PlayerId },
    MakePick { lobby_id: DraftLobbyId, player_id: PlayerId, pick: DraftItemId },
    BlockForUpdate { lobby_id: DraftLobbyId, player_id: PlayerId, game_state: u64 },
}

pub enum LobbyManagerResponse {
    LobbyErrorMsg(String),
    LobbyCreated(DraftLobbyId),
    LobbyJoined { lobby_id: DraftLobbyId, player_id: PlayerId },
    LobbyStarted,
    PickMade,
    LobbyState(LobbyStateForPlayer),
    UpdateReady,
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
            if let LobbyManagerRequest::BlockForUpdate { lobby_id, player_id, game_state } = task.request {
                self.add_listener_for(lobby_id, player_id, game_state, task.response_channel);
                continue;
            }
            match task.response_channel.send(self.process_request(task.request)) {
                Ok(_) => {}
                Err(_) => log::warn!("Could not respond to request, as receiver dropped."),
            }
        }
    }

    fn add_listener_for(&mut self, lobby_id: DraftLobbyId, player_id: PlayerId, game_state: u64, listener: tokio::sync::oneshot::Sender<LobbyManagerResponse>) {
        match self.active_lobbies.get_mut(&lobby_id) {
            None => {
                log::warn!("Tried to poll a lobby that doesn't exist, returning immediately");
                match listener.send(LobbyManagerResponse::LobbyErrorMsg("Polled a lobby that doesn't exist".to_string())) {
                    Ok(_) => {}
                    Err(_) => log::warn!("Receiver dropped"),
                }
            }
            Some(lobby) => {
                match lobby.add_listener(player_id, game_state, listener) {
                    Ok(_) => {}
                    Err(e) => log::warn!("Failed to add listener {e}"),
                }
            }
        }
    }

    fn process_request(&mut self, request: LobbyManagerRequest) -> LobbyManagerResponse {
        match request {
            LobbyManagerRequest::CreateLobby => LobbyManagerResponse::LobbyCreated(self.create_lobby()),
            LobbyManagerRequest::JoinLobby { lobby_id, player_name } => self.join_lobby(lobby_id, player_name),
            LobbyManagerRequest::StartLobby { lobby_id } => self.start_lobby(lobby_id),
            LobbyManagerRequest::GetLobbyState { lobby_id, player_id } => match self.get_lobby_state(lobby_id, player_id) {
                Ok(s) => LobbyManagerResponse::LobbyState(s),
                Err(e) => {
                    log::error!("Error retrieving state {e}");
                    LobbyManagerResponse::LobbyErrorMsg("Error fetching state".to_string())
                }
            },
            LobbyManagerRequest::MakePick { lobby_id, player_id, pick } => self.make_pick(lobby_id, player_id, pick),
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

    fn join_lobby(&mut self, lobby_id: DraftLobbyId, player_name: String) -> LobbyManagerResponse {
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
    }

    fn start_lobby(&mut self, lobby_id: DraftLobbyId) -> LobbyManagerResponse {
        let start = match self.active_lobbies.get_mut(&lobby_id) {
            Some(lobby) => lobby.start(self.draft_database.get_item_list()),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Lobby not found"))
        };
        match start {
            Ok(_) => {
                log::info!("Started draft in lobby {lobby_id}");
                LobbyManagerResponse::LobbyStarted
            }
            Err(e) => {
                log::warn!("Failed to start lobby {lobby_id}: {e}");
                LobbyManagerResponse::LobbyErrorMsg("Lobby did not start".to_string())
            }
        }
    }

    fn get_lobby_state(&self, lobby_id: DraftLobbyId, player_id: PlayerId) -> io::Result<LobbyStateForPlayer> {
        let lobby = self.active_lobbies.get(&lobby_id);
        if lobby.is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Couldn't find lobby"));
        }
        let lobby = lobby.unwrap();

        // If the draft is still waiting to start, show joining players and open slots
        let (joining_players, open_slots) = match lobby.draft_has_started() {
            true => (vec![], vec![]),
            false => {
                let joining_players = lobby.get_player_names();
                let num_open_slots = MAX_LOBBY_CAPACITY - joining_players.len();
                let open_slots = vec!["Open Slot".to_string(); num_open_slots];
                (joining_players, open_slots)
            }
        };

        let (pending_picks, allocated_picks) = match lobby.get_player_draft_state(&player_id) {
            None => (vec![], vec![]),
            Some(state) => {
                let allocated_picks: Vec<String> = state.allocated_items.iter()
                    .map(|pick_id| self.draft_database.get_item_by_id(&pick_id).unwrap().get_template().clone())
                    .collect();
                let pending_picks: Vec<(DraftItemId, String)> = match lobby.get_current_pack_contents_for_player(&player_id) {
                    Some(pack_contents) => {
                        let mut v: Vec<(DraftItemId, String)> = Vec::new();
                        for &item_id in pack_contents.iter() {
                            v.push((item_id, self.draft_database.get_item_by_id(&item_id).unwrap().get_template().clone()))
                        }
                        v
                    }
                    None => vec![]
                };
                (pending_picks, allocated_picks)
            }
        };

        let game_state = lobby.compute_state(&player_id);

        return Ok(LobbyStateForPlayer {
            lobby_id,
            player_id,
            joining_players,
            open_slots,
            pending_picks,
            allocated_picks,
            game_state,
        });
    }

    fn make_pick(&mut self, lobby_id: DraftLobbyId, player_id: PlayerId, pick_id: DraftItemId) -> LobbyManagerResponse {
        let lobby = self.active_lobbies.get_mut(&lobby_id);
        if lobby.is_none() {
            return LobbyManagerResponse::LobbyErrorMsg("Lobby doesn't exist".to_string());
        }
        match lobby.unwrap().make_pick(player_id, pick_id) {
            Ok(_) => {
                log::info!("Lobby {lobby_id} Player {player_id} Picked {pick_id}");
                LobbyManagerResponse::PickMade
            }
            Err(e) => LobbyManagerResponse::LobbyErrorMsg("Error making pick".to_string())
        }
    }
}