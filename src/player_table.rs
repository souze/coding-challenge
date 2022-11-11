use rand::seq::SliceRandom;
use std::collections::HashMap;

use druid::Color;
use tokio::sync::mpsc;

use crate::controller::ControllerToPlayerMsg;

pub struct PlayerTable {
    players: Vec<PlayerInfo>,
    current_index: usize,
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
            current_index: 0,
            paint_bucket: PaintBucket::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    pub fn current(&self) -> Option<&PlayerInfo> {
        self.players.get(self.current_index)
    }

    pub fn add_new_player(&mut self, name: String, channel: mpsc::Sender<ControllerToPlayerMsg>) {
        self.remove_player(&name);
        self.players.push(PlayerInfo {
            color: self.paint_bucket.get(&name),
            name,
            tx: channel,
        });
    }

    pub fn remove_current(&mut self) {
        self.players.remove(self.current_index);
        if self.current_index >= self.players.len() {
            self.current_index = 0;
        }
    }

    pub fn remove_player(&mut self, name: &str) {
        let mut new_players = Vec::<PlayerInfo>::new();
        for (index, player) in self.players.iter().enumerate() {
            if player.name == name {
                // If we're removing a player before the current player, the current player moves one left
                if index < self.current_index {
                    self.current_index -= 1;
                }
            } else {
                new_players.push(player.clone());
            }
        }
        self.players = new_players;
    }

    pub fn advance_player(&mut self) -> Option<&PlayerInfo> {
        self.current_index += 1;
        if self.current_index >= self.players.len() {
            self.current_index = 0;
        }
        self.current()
    }

    pub fn iter(&self) -> std::slice::Iter<PlayerInfo> {
        self.players.iter()
    }

    pub fn shuffle(&mut self) {
        let mut rng = rand::thread_rng();
        self.players.shuffle(&mut rng);
        self.current_index = 0;
    }
}

#[derive(Clone)]
pub struct PlayerInfo {
    pub name: String,
    pub color: druid::Color,
    pub tx: mpsc::Sender<ControllerToPlayerMsg>,
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
            Some(color) => color.clone(),
            None => {
                // After all the colors are taken, everyone gets gray
                let c = self.free_paints.pop().unwrap_or(Color::GRAY);
                self.taken_paints.insert(name.to_string(), c.clone());
                c
            }
        }
    }
}

mod test {
    #[allow(unused_imports)]
    use super::*;

    #[allow(dead_code)]
    struct StrSeq {
        i: u32,
    }
    impl StrSeq {
        #[allow(dead_code)]
        fn n(&mut self) -> String {
            self.i += 1;
            self.i.to_string()
        }
    }

    #[test]
    fn flow() {
        let (tx, _) = mpsc::channel::<ControllerToPlayerMsg>(128);
        let mut str_seq = StrSeq { i: 0 };
        let mut n = || str_seq.n();

        let mut p = PlayerTable::new();
        p.add_new_player(n(), tx.clone());
        p.add_new_player(n(), tx.clone());
        p.add_new_player(n(), tx.clone());
        p.add_new_player(n(), tx.clone());
        assert_eq!(p.current().unwrap().name, "1");
        p.advance_player();
        assert_eq!(p.current().unwrap().name, "2");
        p.remove_current();
        assert_eq!(p.current().unwrap().name, "3");
        p.remove_player("3");
        assert_eq!(p.current().unwrap().name, "4");
        p.add_new_player(n(), tx);
        assert_eq!(p.current().unwrap().name, "4");
        p.advance_player();
        assert_eq!(p.current().unwrap().name, "5");
        p.remove_current();
        assert_eq!(p.current().unwrap().name, "1");
        p.advance_player();
        assert_eq!(p.current().unwrap().name, "4");
    }
}
