use std::collections::HashMap;
use std::io;

use rand::{RngCore};

use crate::draft_database::DraftDb;
use crate::draft_engine;
use crate::draft_engine::{DraftDeadline, DraftItemId, GameState, PlayerId};

pub type DraftLobbyId = u64;

#[derive(Debug)]
pub struct LobbyStateForPlayer {
    pub lobby_id: DraftLobbyId,
    pub player_id: PlayerId,
    pub joining_players: Vec<String>,
    pub open_slots: Vec<String>,
    pub pending_picks: Vec<(DraftItemId, String, String)>,
    pub allocated_picks: Vec<(String, String)>,
    pub game_state: GameState,
    pub draft_is_finished: bool,
    pub time_to_pick_s: Option<u64>,
    pub draft_order: Vec<String>,
    pub rounds_and_picks: (usize, usize, usize, usize),
    pub raw_picks: Vec<String>,
}

pub enum LobbyManagerRequest {
    CreateLobby { set_name: String },
    JoinLobby { lobby_id: DraftLobbyId, player_name: String },
    StartLobby { lobby_id: DraftLobbyId },
    GetLobbyState { lobby_id: DraftLobbyId, player_id: PlayerId },
    MakePick { lobby_id: DraftLobbyId, player_id: PlayerId, pick: DraftItemId },
    BlockForUpdate { lobby_id: DraftLobbyId, player_id: PlayerId, game_state: GameState },
    EnforceDeadline { lobby_id: DraftLobbyId, round_number: usize, pick_number: usize },
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
    draft_database: DraftDb,
    active_lobbies: HashMap<DraftLobbyId, draft_engine::DraftLobby>,
    task_queue: tokio::sync::mpsc::Receiver<LobbyManagerTask>,
    self_queue: tokio::sync::mpsc::Sender<LobbyManagerTask>,
    scheduling: timer::Timer,
}

impl LobbyManager {
    pub fn new(task_queue: tokio::sync::mpsc::Receiver<LobbyManagerTask>,
               self_queue: tokio::sync::mpsc::Sender<LobbyManagerTask>,
               draft_database: DraftDb) -> LobbyManager {
        LobbyManager {
            draft_database,
            active_lobbies: HashMap::new(),
            task_queue,
            self_queue,
            scheduling: timer::Timer::new(),
        }
    }

    pub fn run(&mut self) -> () {
        while let Some(task) = self.task_queue.blocking_recv() {
            if let LobbyManagerRequest::BlockForUpdate { lobby_id, player_id, game_state } = task.request {
                self.add_listener_for(lobby_id, player_id, game_state, task.response_channel);
                continue;
            }
            if let LobbyManagerRequest::EnforceDeadline { lobby_id, round_number, pick_number } = task.request {
                match self.enforce_deadline(lobby_id, round_number, pick_number) {
                    Ok(_) => {}
                    Err(e) => log::error!("Failed to enforce a lobby deadline: {e}")
                };
                continue;
            }
            match task.response_channel.send(self.process_request(task.request)) {
                Ok(_) => {}
                Err(_) => log::warn!("Could not respond to request, as receiver dropped."),
            }
        }
    }

    fn add_listener_for(&mut self, lobby_id: DraftLobbyId, player_id: PlayerId, game_state: GameState, listener: tokio::sync::oneshot::Sender<LobbyManagerResponse>) {
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
            LobbyManagerRequest::CreateLobby {set_name} => match self.create_lobby(set_name) {
                Some(lobby_id) => LobbyManagerResponse::LobbyCreated(lobby_id),
                None => LobbyManagerResponse::LobbyErrorMsg("Unknown draft set".to_string())
            },
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

    fn create_lobby(&mut self, set_name: String) -> Option<DraftLobbyId> {
        let lobby_id = self.generate_lobby_id();
        match self.draft_database.get_set(&set_name) {
            Some(_) => {
                log::info!("Creating lobby {lobby_id} for set {set_name}");
                self.active_lobbies.insert(lobby_id, draft_engine::DraftLobby::new(set_name, MAX_LOBBY_CAPACITY));
                Some(lobby_id)
            },
            None => {
                log::error!("Got a request for unknown draft set {set_name}");
                None
            }
        }
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
            Some(lobby) => {
                let draft_set = self.draft_database.get_set(lobby.get_set()).unwrap();
                let draft_items = draft_set.get_item_list();
                lobby.start(&draft_items)
            }
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Lobby not found"))
        };
        match start {
            Ok(deadline) => {
                log::info!("Started draft in lobby {lobby_id}");
                self.enqueue_deadline(lobby_id, deadline);
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
        let draft_set = self.draft_database.get_set(lobby.get_set()).unwrap();

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

        let (pending_picks, allocated_picks, raw_picks) = match lobby.get_player_draft_state(&player_id) {
            None => (vec![], vec![], vec![]),
            Some(state) => {
                let allocated_picks: Vec<(String, String)> = state.allocated_items.iter()
                    .map(|pick_id| {
                        let template = draft_set.get_item_by_id(&pick_id).unwrap().get_template().clone();
                        let stats = draft_set.get_item_by_id(&pick_id).unwrap().get_stats().clone();
                        (template, stats)
                    })
                    .collect();
                let raw_picks: Vec<String> = state.allocated_items.iter()
                    .map(|pick_id| draft_set.get_item_by_id(&pick_id).unwrap().get_raw().clone())
                    .collect();
                let pending_picks: Vec<(DraftItemId, String, String)> = match lobby.get_current_pack_contents_for_player(&player_id) {
                    Some(pack_contents) => {
                        let mut v: Vec<(DraftItemId, String, String)> = Vec::new();
                        for &item_id in pack_contents.iter() {
                            v.push((item_id, draft_set.get_item_by_id(&item_id).unwrap().get_template().clone(),
                                    draft_set.get_item_by_id(&item_id).unwrap().get_stats().clone()))
                        }
                        v
                    }
                    None => vec![]
                };
                (pending_picks, allocated_picks, raw_picks)
            }
        };

        let game_state = lobby.compute_state(&player_id);
        let draft_is_finished = lobby.draft_is_finished();

        let time_to_pick_s = lobby.get_next_deadline_for_player(&player_id)
            .map(|deadline| deadline.checked_duration_since(std::time::Instant::now()))
            .flatten()
            .map(|remaining_time| remaining_time.as_secs());

        let draft_order = lobby.get_draft_order();
        let rounds_and_picks = lobby.get_draft_progress_for_player(&player_id)
            .unwrap_or((0, 0, 0, 0));

        return Ok(LobbyStateForPlayer {
            lobby_id,
            player_id,
            joining_players,
            open_slots,
            pending_picks,
            allocated_picks,
            game_state,
            draft_is_finished,
            time_to_pick_s,
            draft_order,
            rounds_and_picks,
            raw_picks,
        });
    }

    fn make_pick(&mut self, lobby_id: DraftLobbyId, player_id: PlayerId, pick_id: DraftItemId) -> LobbyManagerResponse {
        let lobby = self.active_lobbies.get_mut(&lobby_id);
        if lobby.is_none() {
            return LobbyManagerResponse::LobbyErrorMsg("Lobby doesn't exist".to_string());
        }
        match lobby.unwrap().make_pick(player_id, pick_id) {
            Ok(maybe_deadline) => {
                if maybe_deadline.is_some() {
                    self.enqueue_deadline(lobby_id, maybe_deadline.unwrap());
                }
                LobbyManagerResponse::PickMade
            }
            Err(e) => {
                log::warn!("Pick error @ [Lobby {lobby_id} Player {player_id} Picked {pick_id}]: {e}");
                LobbyManagerResponse::LobbyErrorMsg("Error making pick".to_string())
            }
        }
    }

    fn generate_lobby_id(&self) -> DraftLobbyId {
        let id: DraftLobbyId = rand::thread_rng().next_u64();
        if self.active_lobbies.contains_key(&id) {
            self.generate_lobby_id()
        } else {
            id
        }
    }

    fn enqueue_deadline(&mut self, lobby_id: DraftLobbyId, deadline: DraftDeadline) {
        let channel = self.self_queue.clone();
        let delay = deadline.deadline.checked_duration_since(std::time::Instant::now()).unwrap_or(std::time::Duration::ZERO);
        let delay = chrono::Duration::from_std(delay).unwrap();
        // Remember to ignore the guard so that dropping the guard doesn't cancel execution
        self.scheduling.schedule_with_delay(delay, move || {
            let (tx, _) = tokio::sync::oneshot::channel();
            let task = LobbyManagerTask {
                request: LobbyManagerRequest::EnforceDeadline { lobby_id, round_number: deadline.round_number, pick_number: deadline.pick_number },
                response_channel: tx,
            };
            let result = channel.blocking_send(task);
            if let Err(e) = result {
                log::error!("Unexpected scheduling error {e}")
            }
        }).ignore();
    }

    fn enforce_deadline(&mut self, lobby_id: DraftLobbyId, round_idx: usize, pick_idx: usize) -> io::Result<()> {
        if !self.active_lobbies.contains_key(&lobby_id) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Couldn't find lobby"));
        }
        let lobby = self.active_lobbies.get_mut(&lobby_id).unwrap();
        match lobby.enforce_deadline(round_idx, pick_idx)? {
            Some(new_deadline) => self.enqueue_deadline(lobby_id, new_deadline),
            _ => ()
        }
        Ok(())
    }
}