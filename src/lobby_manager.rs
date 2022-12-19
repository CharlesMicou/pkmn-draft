use std::collections::HashMap;
use std::io;
use crate::draft_engine;
use crate::draft_engine::DraftLobby;

struct DraftDatabase {}

pub type DraftLobbyId = u64;

pub enum LobbyManagerRequest {
    Foo(i32)
}

pub enum LobbyManagerResponse {
    Bar(u32)
}

pub struct LobbyManagerTask {
    request: LobbyManagerRequest,
    response_channel: tokio::sync::oneshot::Sender<LobbyManagerResponse>,
}

const MAX_LOBBY_CAPACITY: usize = 6;

impl DraftDatabase {
    pub fn get_item_list(&self) -> Vec<draft_engine::DraftItemId> {
        return vec!();
    }
}

pub struct LobbyManager {
    draft_database: DraftDatabase,
    active_lobbies: HashMap<DraftLobbyId, draft_engine::DraftLobby>,
    next_lobby_id: DraftLobbyId,
    task_queue: tokio::sync::mpsc::Receiver<LobbyManagerTask>,
}

impl LobbyManager {
    pub fn new(request_queue: tokio::sync::mpsc::Receiver<LobbyManagerTask>) -> LobbyManager {
        LobbyManager {
            draft_database: DraftDatabase {},
            active_lobbies: HashMap::new(),
            next_lobby_id: 0,
            task_queue: request_queue,
        }
    }

    pub fn run(&mut self) -> () {
        while let Some(task) = self.task_queue.blocking_recv() {
            match task.response_channel.send(self.process_request(task.request)) {
                Ok(_) => {},
                Err(_) => println!("Receiver dropped"),
            }
        }
    }

    fn process_request(&mut self, request: LobbyManagerRequest) -> LobbyManagerResponse {
        LobbyManagerResponse::Bar(123)
    }

    pub fn create_lobby(&mut self) -> DraftLobbyId {
        let lobby_id = self.next_lobby_id;
        self.active_lobbies.insert(lobby_id, draft_engine::DraftLobby::new(MAX_LOBBY_CAPACITY));
        self.next_lobby_id += 1;
        lobby_id
    }

    pub fn get_lobby(&self, lobby_id: DraftLobbyId) -> Option<&draft_engine::DraftLobby> {
        self.active_lobbies.get(&lobby_id)
    }

    pub fn start_draft(&mut self, lobby_id: DraftLobbyId) -> io::Result<()> {
        match self.active_lobbies.get_mut(&lobby_id) {
            Some(lobby) => lobby.start(&self.draft_database.get_item_list()),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Lobby not found"))
        }
    }
}