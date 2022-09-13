use std::any::Any;

use crate::gametraits::{self, GameTrait, PlayerMoveResult, User};
use druid::{
    piet::{Text, TextLayoutBuilder},
    Color, FontFamily, RenderContext,
};
use log::{debug, info};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct PlayerState {
    num: u32,
}

#[derive(Debug, Deserialize)]
pub struct PlayerMove {
    add: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    count: u32,
}

impl Default for Game {
    fn default() -> Self {
        info!("New game created");
        Game { count: 0 }
    }
}

impl gametraits::GameTrait for Game {
    fn player_moves(
        &mut self,
        user: &User,
        player_move: gametraits::PlayerMove,
    ) -> PlayerMoveResult {
        debug!("{user:?} made a move {player_move:?}");
        match gametraits::to_player_move::<PlayerMove>(&player_move) {
            None => PlayerMoveResult::InvalidFormat,
            Some(mov) => make_move(self, user, mov),
        }
    }

    fn get_player_state(&self, _user: &User) -> gametraits::PlayerGameState {
        gametraits::to_game_state(PlayerState { num: self.count })
    }

    fn paint(&self, ctx: &mut druid::PaintCtx) {
        let my_debug_str = format!("num: {}", self.count);

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

    fn reset(&mut self) {
        *self = Game::default();
    }
}

pub fn make_ptr() -> Box<dyn GameTrait> {
    Box::new(Game::default())
}

fn make_move(state: &mut Game, _user: &User, p_move: PlayerMove) -> PlayerMoveResult {
    state.count += p_move.add;
    PlayerMoveResult::Ok
}
