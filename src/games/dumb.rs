use std::any::Any;

use code_challenge_game_types::gametraits::{
    self, GameTrait, PlayerMoveResult, PlayerTurn, TurnToken, User,
};

use code_challenge_game_types::TurnTracker;

use druid::{
    piet::{Text, TextLayoutBuilder},
    Color, FontFamily, RenderContext,
};
use log::debug;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Eq, PartialEq)]
pub struct PlayerState {
    num: u32,
}

#[derive(Debug, Deserialize)]
pub struct PlayerMove {
    add: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    count: PlayerState,
    players: TurnTracker,
}

impl Game {
    pub fn new() -> Self {
        Game {
            count: PlayerState { num: 0 },
            players: TurnTracker::new(Vec::new()),
        }
    }
}

impl gametraits::GameTrait for Game {
    fn player_moves(
        &mut self,
        token: TurnToken,
        player_move: gametraits::PlayerMove,
    ) -> PlayerMoveResult {
        let user = &token.user;
        debug!("{user:?} made a move {player_move:?}");
        match gametraits::to_player_move::<PlayerMove>(&player_move) {
            None => {
                self.players.remove_player(&user.name);
                match self.players.advance_player() {
                    Some(p) => PlayerMoveResult::InvalidFormat(Some(PlayerTurn {
                        token: TurnToken { user: p },
                        state: gametraits::to_game_state(&self.count),
                    })),
                    None => PlayerMoveResult::InvalidFormat(None),
                }
            }
            Some(mov) => {
                make_move(self, user, mov);
                let next_player = self.players.advance_player().unwrap();
                PlayerMoveResult::Ok(PlayerTurn {
                    token: TurnToken { user: next_player },
                    state: gametraits::to_game_state(&self.count),
                })
            }
        }
    }

    fn player_connected(&mut self, user: User) {
        self.players.add_player(user);
    }
    fn player_disconnected(&mut self, user: &str) {
        self.players.remove_player(user);
    }

    // fn get_player_state(&self, _user: &User) -> gametraits::PlayerGameState {
    //     gametraits::to_game_state(PlayerState { num: self.count })
    // }

    fn paint(&self, ctx: &mut druid::PaintCtx) {
        let my_debug_str = format!("num: {:?}", self.count);

        // This is the builder-style way of drawing text.
        let text = ctx.text();
        let layout = text
            .new_text_layout(my_debug_str)
            .font(FontFamily::SERIF, 24.0)
            .text_color(Color::rgb8(180, 180, 180))
            .build()
            .unwrap();
        ctx.draw_text(&layout, (100.0, 25.0));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn eq(&self, other: &dyn GameTrait) -> bool {
        other
            .as_any()
            .downcast_ref::<Game>()
            .map_or(false, |v| v == self)
    }

    fn current_player_disconnected(&mut self, turn_token: TurnToken) -> Option<PlayerTurn> {
        self.players.remove_player(&turn_token.user.name);
        match self.players.advance_player() {
            None => None,
            Some(user) => Some(PlayerTurn {
                token: TurnToken { user },
                state: gametraits::to_game_state(&self.count),
            }),
        }
    }

    fn try_start_game(&mut self) -> Option<PlayerTurn> {
        Some(PlayerTurn {
            token: TurnToken {
                user: self.players.advance_player().unwrap(),
            },
            state: gametraits::to_game_state(&self.count),
        })
    }

    fn reset(&mut self, _users: Vec<User>) {
        todo!()
    }
}

pub fn make_ptr(players: Vec<gametraits::User>) -> Box<dyn GameTrait> {
    Box::new(Game {
        count: PlayerState { num: 0 },
        players: TurnTracker::new(players),
    })
}

fn make_move(state: &mut Game, _user: &User, p_move: PlayerMove) {
    state.count.num += p_move.add;
}
