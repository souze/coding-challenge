use crate::gametraits;

use gametraits::User;
use itertools::enumerate;
use itertools::Itertools;
use log::debug;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnTracker {
    players: Vec<User>,
    current_player_index: usize,
}

impl TurnTracker {
    fn player_string(&self) -> String {
        let mut players: String = String::new();
        for (i, User { name, .. }) in enumerate(&self.players) {
            if i == self.current_player_index {
                players += format!("*{name}").as_str();
            } else {
                players += name.to_string().as_str();
            }
        }
        players
    }

    pub fn new(players: Vec<User>) -> Self {
        debug!("Creating turn tracker, with users {players:?}");
        Self {
            players,
            current_player_index: 0,
        }
    }

    pub fn remove_player(&mut self, username: &str) {
        let p_str = self.player_string();
        debug!("Removing player {username}, left: {p_str}");
        let (i, _) = self
            .players
            .iter()
            .find_position(|u| u.name == username)
            .unwrap();

        if i <= self.current_player_index {
            // Remove player earlier in the list
            if self.players.is_empty() {
                self.current_player_index = 0;
            } else if i == 0 {
                self.current_player_index = self.players.len() - 1;
            } else {
                self.current_player_index -= 1;
            }
        }
        self.players = self
            .players
            .iter()
            .filter(|u| u.name != username)
            .map(Clone::clone)
            .collect();
    }

    pub fn add_player(&mut self, user: User) {
        let p_str = self.player_string();
        debug!("Adding player {user:?}, new: {p_str}");
        self.players.push(user);
    }

    pub fn advance_player(&mut self) -> Option<User> {
        let p_str = self.player_string();
        debug!("Advancing player, new: {p_str}");
        if self.players.is_empty() {
            return None;
        }
        self.current_player_index = (self.current_player_index + 1) % self.players.len();
        self.players
            .get(self.current_player_index)
            .map(Clone::clone)
    }

    pub fn num_players(&self) -> usize {
        self.players.len()
    }
}
