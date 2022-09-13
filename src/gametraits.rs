use std::{any::Any, fmt::Debug};

use druid::Data;
use serde::{Deserialize, Serialize};

use crate::messages;

#[derive(Debug)]
pub struct PlayerGameState {
    pub serialized: String,
}

pub fn to_game_state<S>(state: S) -> PlayerGameState
where
    S: Serialize,
{
    PlayerGameState {
        serialized: serde_json::to_string(&messages::YourTurn::YourTurn(state)).unwrap() + "\n",
    }
}

#[derive(Debug)]
pub struct PlayerMove {
    pub serialized: String,
}

pub fn to_player_move<'a, MoveType: Deserialize<'a>>(p_move: &'a PlayerMove) -> Option<MoveType> {
    serde_json::from_str::<messages::Move<MoveType>>(&p_move.serialized)
        .map(|messages::Move::Move(m)| Some(m))
        .unwrap_or(None)
}

pub trait GameTrait: dyn_clone::DynClone + Send + Debug {
    fn player_moves(&mut self, user: &User, player_move: PlayerMove) -> PlayerMoveResult;

    fn get_player_state(&self, user: &User) -> PlayerGameState;

    fn reset(&mut self);

    fn paint(&self, ctx: &mut druid::PaintCtx);

    fn as_any(&self) -> &dyn Any;
    fn eq(&self, other: &dyn GameTrait) -> bool;
}

dyn_clone::clone_trait_object!(GameTrait);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Data)]
pub struct User {
    pub name: String,
    #[serde(skip_serializing)]
    pub color: druid::piet::Color,
}

#[derive(PartialEq, Eq, Debug)]
pub enum PlayerMoveResult {
    Ok,
    Win,
    Draw,
    InvalidMove,
    InvalidFormat,
}
