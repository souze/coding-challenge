use log::debug;
use std::collections::HashMap;

use druid::Color;
use tokio::sync::mpsc;

use crate::controller::ControllerToPlayerMsg;

pub struct PlayerTable {
    players: Vec<PlayerInfo>,
    paint_bucket: PaintBucket,
}

impl Default for PlayerTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerTable {
    pub fn new() -> Self {
        Self {
            players: Vec::new(),
            paint_bucket: PaintBucket::new(),
        }
    }

    fn debug_print(&self, msg: &str) {
        let p = &self.players;
        debug!("{p:?}: {msg}");
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    pub fn add_new_player(
        &mut self,
        name: String,
        channel: mpsc::Sender<ControllerToPlayerMsg>,
    ) -> &PlayerInfo {
        self.remove_player(&name);
        self.players.push(PlayerInfo {
            color: self.paint_bucket.get(&name),
            name,
            tx: channel,
        });
        self.debug_print("Added player");
        self.players.last().unwrap()
    }

    pub fn remove_player(&mut self, name: &str) -> bool {
        let mut new_players = Vec::<PlayerInfo>::new();
        let mut was_removed = false;
        for player in self.players.iter() {
            if player.name == name {
                // If we're removing a player before the current player, the current player moves one left
                was_removed = true;
            } else {
                new_players.push(player.clone());
            }
        }
        self.players = new_players;
        self.debug_print("Removed player");
        was_removed
    }

    pub fn iter(&self) -> std::slice::Iter<PlayerInfo> {
        self.players.iter()
    }

    pub(crate) fn get(&self, name: &str) -> Option<&PlayerInfo> {
        self.players.iter().find(|p| p.name == name)
    }
}

#[derive(Clone)]
pub struct PlayerInfo {
    pub name: String,
    pub color: druid::Color,
    pub tx: mpsc::Sender<ControllerToPlayerMsg>,
}

impl std::fmt::Debug for PlayerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PlayerInfo({}, {:?})", self.name, self.color)
    }
}

struct PaintBucket {
    free_paints: Vec<druid::Color>,
    taken_paints: HashMap<String, druid::Color>,
}

impl PaintBucket {
    fn new() -> Self {
        Self {
            taken_paints: HashMap::new(),
            free_paints: vec![
                druid::Color::rgb8(0, 0, 0),
                druid::Color::rgb8(128, 128, 128),
                druid::Color::rgb8(0, 0, 128),
                druid::Color::rgb8(255, 215, 180),
                druid::Color::rgb8(128, 128, 0),
                druid::Color::rgb8(170, 255, 195),
                druid::Color::rgb8(128, 0, 0),
                druid::Color::rgb8(255, 250, 200),
                druid::Color::rgb8(170, 110, 40),
                druid::Color::rgb8(220, 190, 255),
                druid::Color::rgb8(0, 128, 128),
                druid::Color::rgb8(250, 190, 212),
                druid::Color::rgb8(210, 245, 60),
                druid::Color::rgb8(240, 50, 230),
                druid::Color::rgb8(70, 240, 240),
                druid::Color::rgb8(145, 30, 180),
                druid::Color::rgb8(245, 130, 48),
                druid::Color::rgb8(0, 130, 200),
                druid::Color::rgb8(255, 225, 25),
                druid::Color::rgb8(60, 180, 75),
                druid::Color::rgb8(230, 25, 75),
            ],
        }
    }

    fn get(&mut self, name: &String) -> Color {
        match self.taken_paints.get(name) {
            Some(color) => *color,
            None => {
                // After all the colors are taken, everyone gets gray
                let c = self.free_paints.pop().unwrap_or(Color::GRAY);
                self.taken_paints.insert(name.to_string(), c);
                c
            }
        }
    }
}
