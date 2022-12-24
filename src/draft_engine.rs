use std::io;
use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
use rand::{RngCore, thread_rng, Rng};
use crate::{lobby_manager, LobbyManagerResponse};

extern crate rand;

pub type DraftItemId = u64;
pub type PackId = u64;
pub type PlayerId = u64;
pub type PackContents = Vec<DraftItemId>;
pub type ResponseChannel = tokio::sync::oneshot::Sender<lobby_manager::LobbyManagerResponse>;

pub struct PlayerState {
    pub allocated_items: Vec<DraftItemId>,
    pub pending_packs: VecDeque<PackId>,
}

pub struct DraftState {
    players: HashMap<PlayerId, PlayerState>,
    turn_order: Vec<PlayerId>,
    packs_by_round: Vec<HashMap<PackId, PackContents>>,
    current_round_idx: usize,
    draft_direction: bool,
}

pub struct UpdateListener {
    response_channel: Option<ResponseChannel>,
    game_state: u64,
}

impl UpdateListener {
    pub fn flush(&mut self) -> io::Result<()> {
        let channel = self.response_channel.take();
        if channel.is_none() {
            return Ok(());
        }
        match channel.unwrap().send(LobbyManagerResponse::UpdateReady) {
            Ok(_) => Ok(()),
            Err(_) => {
                // No need to log this: actually happens if users close browser.
                return Err(io::Error::new(io::ErrorKind::ConnectionRefused, "Receiver has dropped"));
            }
        }
    }
}

pub struct DraftLobby {
    player_capacity: usize,
    draft_state: Option<DraftState>,
    joined_players: HashMap<PlayerId, String>,
    listeners: HashMap<PlayerId, Vec<UpdateListener>>,
}

impl DraftLobby {
    pub fn new(player_capacity: usize) -> DraftLobby {
        return DraftLobby {
            player_capacity,
            draft_state: None,
            joined_players: HashMap::new(),
            listeners: HashMap::new(),
        };
    }

    pub fn add_player(&mut self, name: String) -> io::Result<PlayerId> {
        // Validation
        if self.draft_state.is_some() {
            return Err(io::Error::new(io::ErrorKind::ConnectionRefused, "Game has already started"));
        }
        if self.joined_players.values().find(|&x| x == &name).is_some() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("Player name {} has already joined", name)));
        }
        if self.joined_players.len() >= self.player_capacity {
            return Err(io::Error::new(io::ErrorKind::ConnectionRefused, "Lobby full"));
        }
        let id = self.generate_player_id();
        self.joined_players.insert(id, name);
        self.listeners.insert(id, vec![]);
        self.check_listeners();
        return Ok(id);
    }

    pub fn add_listener(&mut self, player_id: PlayerId, game_state: u64, response_channel: ResponseChannel) -> io::Result<()> {
        let current_state = self.compute_state(&player_id);
        let mut listener = UpdateListener{response_channel: Some(response_channel), game_state};
        if current_state != game_state {
            return listener.flush()
        }
        if !self.listeners.contains_key(&player_id) {
            return listener.flush()
        }
        self.listeners.get_mut(&player_id).unwrap().push(listener);
        Ok(())
    }

    pub fn start(&mut self, item_list: &Vec<DraftItemId>) -> io::Result<()> {
        if self.draft_state.is_some() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Game has already started"));
        }
        if self.joined_players.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Lobby has no players"));
        }
        let player_ids: Vec<PlayerId> = self.joined_players.keys().cloned().collect();
        let (num_rounds, num_items_in_pack) = get_rounds_and_pack_sizes(player_ids.len());
        let packs = make_random_packs(num_rounds * player_ids.len(), num_items_in_pack, item_list)?;
        self.draft_state = Some(DraftState::new(player_ids, packs, num_rounds));
        self.check_listeners();
        Ok(())
    }

    pub fn get_player_names(&self) -> Vec<String> {
        self.joined_players.values().cloned().collect()
    }

    pub fn draft_has_started(&self) -> bool {
        self.draft_state.is_some()
    }

    pub fn get_player_draft_state(&self, player_id: &PlayerId) -> Option<&PlayerState> {
        if self.draft_state.is_none() {
            return None;
        }
        return self.draft_state.as_ref().unwrap().players.get(player_id);
    }

    pub fn make_pick(&mut self, player_id: PlayerId, picked_item_id: DraftItemId) -> io::Result<()> {
        if self.draft_state.is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Draft hasn't started yet"));
        }
        self.draft_state.as_mut().unwrap().pick(player_id, picked_item_id)?;
        self.check_listeners();
        Ok(())
    }

    pub fn get_current_pack_contents_for_player(&self, player_id: &PlayerId) -> Option<&PackContents> {
        if self.draft_state.is_none() {
            return None;
        }
        let pack_id = self.get_player_draft_state(player_id)
            .map(|player_state| player_state.pending_packs.front())
            .flatten();
        if pack_id.is_none() {
            return None;
        }
        self.draft_state.as_ref()
            .map(|d| d.get_pack_contents(pack_id.unwrap()))
            .flatten()
    }

    fn generate_player_id(&self) -> PlayerId {
        let id: PlayerId = rand::thread_rng().next_u64();
        if self.joined_players.contains_key(&id) {
            self.generate_player_id()
        } else {
            id
        }
    }

    fn force_minimum_allocated(&mut self, minimum_allocated: usize) -> io::Result<()> {
        if self.draft_state.is_none() { return Ok(()); };
        let draft_state = self.draft_state.as_mut().unwrap();
        let round_idx = draft_state.current_round_idx;

        let mut picks_to_make: Vec<(PlayerId, DraftItemId)> = vec!();

        for (&player_id, player_state) in &draft_state.players {
            // Currently assumes this will be called each time, so only checks once
            if player_state.allocated_items.len() < minimum_allocated && !player_state.pending_packs.is_empty() {
                let pack_id = player_state.pending_packs.get(0).unwrap();
                let pack_contents = draft_state
                    .packs_by_round.get(round_idx).unwrap()
                    .get(pack_id).unwrap();
                let &random_pick = pack_contents.get(0).unwrap();
                picks_to_make.push((player_id, random_pick));
            }
        }

        for (player_id, random_pick) in picks_to_make {
            draft_state.pick(player_id, random_pick)?;
        }
        self.check_listeners();
        Ok(())
    }

    fn check_listeners(&mut self) {
        let current_states: Vec<(PlayerId, u64)> = self.listeners.keys()
            .map(|&player_id| (player_id, self.compute_state(&player_id)))
            .collect();

        for (player_id, current_state) in current_states {
            let listener_list = self.listeners.get_mut(&player_id).unwrap();
            for listener in listener_list.iter_mut() {
                if listener.game_state != current_state {
                    listener.flush();
                }
            }
            listener_list.retain(|listener| listener.game_state == current_state);
        }
        // for each player_id, list<listener> pair:
        //  compute the state for that player id
        //  for each item in the list of listeners:
        //    check to see if the state matches. if it doesn't then remove the item from the list
        //    and complete the future
    }

    pub fn compute_state(&self, player_id: &PlayerId) -> u64 {
        let num_players = self.joined_players.len() as u64;
        if !self.draft_has_started() {
            return num_players * 1024 * 1024;
        }
        let player_data = self.draft_state.as_ref().unwrap().players.get(player_id);
        if player_data.is_none() {
            return 0;
        }
        let player_data = player_data.unwrap();
        let has_pending_packs = !player_data.pending_packs.is_empty() as u64;
        let num_drafted_so_far = player_data.allocated_items.len() as u64;
        return num_drafted_so_far + 1024 * has_pending_packs + 1024 * 1024 * num_players
    }
}

pub fn make_random_packs(num_packs: usize, pack_size: usize, item_list: &Vec<DraftItemId>) -> io::Result<Vec<PackContents>> {
    let num_unique_items_required = num_packs * pack_size;
    if num_unique_items_required > item_list.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Requested number of packs: {}. Number of unique items: {}", num_unique_items_required, item_list.len())));
    }
    let mut item_indices: Vec<usize> = (0..item_list.len()).collect();
    rand::thread_rng().shuffle(&mut item_indices);

    let mut completed_packs = vec!();
    let mut i = 0;
    for _ in 0..num_packs {
        let mut current_pack = vec!();
        for _ in 0..pack_size {
            let &idx = item_indices.get(i).unwrap();
            let &pack_item = item_list.get(idx).unwrap();
            current_pack.push(pack_item);
            i += 1;
        }
        completed_packs.push(current_pack);
    }

    Ok(completed_packs)
}

pub fn get_rounds_and_pack_sizes(num_players: usize) -> (usize, usize) {
    // need 96 unique sets
    // max capacity: 6 players
    let (num_rounds, pack_size) = match num_players {
        0 => (0, 0),
        1 => (1, 6),
        2 => (4, 4),
        3..=4 => (3, 6),
        5..=6 => (2, 8),
        _ => (0, 0)
    };
    return (num_rounds, pack_size);
}


impl DraftState {
    pub fn new(player_ids: Vec<PlayerId>, mut packs: Vec<PackContents>, num_rounds: usize) -> DraftState {
        let mut players = HashMap::new();
        for player_id in &player_ids {
            players.insert(player_id.clone(), PlayerState { allocated_items: vec!(), pending_packs: VecDeque::new() });
        }

        let mut packs_by_round = vec!();
        let mut current_pack_id = 0;
        for _ in 0..num_rounds {
            let mut round_packs = HashMap::new();
            for _ in 0..player_ids.len() {
                let pack = packs.pop().unwrap();
                round_packs.insert(current_pack_id, pack);
                current_pack_id += 1;
            }
            packs_by_round.push(round_packs);
        }
        // Could add a check here for packs remaining

        let mut draft = DraftState {
            players,
            turn_order: player_ids,
            packs_by_round,
            current_round_idx: 0,
            draft_direction: true,
        };
        draft.set_initial_round_packs().unwrap();
        draft
    }

    pub fn pick(&mut self, player_id: PlayerId, picked_item_id: DraftItemId) -> io::Result<()> {
        if !self.players.contains_key(&player_id) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Couldn't find player"));
        }
        let mut player_state = self.players.get_mut(&player_id).unwrap();
        if player_state.pending_packs.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Player had no packs"));
        }
        let pack_id = player_state.pending_packs.front().unwrap();

        let mut selected_pack = self.packs_by_round
            .get_mut(self.current_round_idx).unwrap()
            .get_mut(pack_id).unwrap();
        let picked_item_idx = selected_pack.iter().position(|x| x == &picked_item_id);
        if picked_item_idx.is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Couldn't find item in pack"));
        }
        let picked_item_idx = picked_item_idx.unwrap();

        player_state.allocated_items.push(picked_item_id);
        let pack_id = player_state.pending_packs.pop_front().unwrap();
        selected_pack.remove(picked_item_idx);

        if !selected_pack.is_empty() {
            let next_player_id = self.next_player_from(player_id)?;
            let mut next_player_state = self.players.get_mut(&next_player_id).unwrap();
            next_player_state.pending_packs.push_back(pack_id);
        }

        Ok(())
    }

    pub fn round_is_done(&self) -> io::Result<bool> {
        let all_packs_empty = self.packs_by_round.get(self.current_round_idx).unwrap()
            .values().all(|pack| pack.is_empty());
        Ok(all_packs_empty)
    }

    pub fn start_next_round(&mut self) -> io::Result<()> {
        self.current_round_idx += 1;
        self.draft_direction = !self.draft_direction;
        self.set_initial_round_packs()
    }


    pub fn next_player_from(&self, player_id: PlayerId) -> io::Result<PlayerId> {
        let player_turn_idx = self.turn_order.iter().position(|x| x == &player_id);
        match player_turn_idx {
            Some(idx) => {
                let next_player_idx = match self.draft_direction {
                    true => (idx + 1) % self.turn_order.len(),
                    false => (idx - 1) % self.turn_order.len(),
                };
                let &next_player_id = self.turn_order.get(next_player_idx).unwrap();
                Ok(next_player_id)
            }
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Couldn't find player"))
        }
    }

    pub fn num_rounds(&self) -> usize {
        return self.packs_by_round.len();
    }

    pub fn rounds_remaining(&self) -> usize {
        return self.num_rounds() - (self.current_round_idx + 1);
    }

    pub fn get_pack_contents(&self, pack_id: &PackId) -> Option<&PackContents> {
        self.packs_by_round.get(self.current_round_idx)
            .map(|r| r.get(pack_id))
            .flatten()
    }

    fn set_initial_round_packs(&mut self) -> io::Result<()> {
        let pack_ids: Vec<PackId> = self.packs_by_round.get(self.current_round_idx).unwrap().keys().cloned().collect();
        for (i, player_state) in self.players.values_mut().into_iter().enumerate() {
            player_state.pending_packs.push_back(pack_ids.get(i).unwrap().clone());
        }
        Ok(())
    }
}
