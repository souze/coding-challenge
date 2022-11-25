use std::{any::Any, fmt::Debug};

use druid::Data;
use serde::{Deserialize, Serialize};

use crate::messages;

#[derive(Clone, PartialEq, Eq, Debug)]
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
    fn player_moves(&mut self, turn_token: TurnToken, player_move: PlayerMove) -> PlayerMoveResult;
    fn current_player_disconnected(&mut self, turn_token: TurnToken) -> Option<PlayerTurn>;

    fn try_start_game(&mut self) -> Option<PlayerTurn>;

    fn player_connected(&mut self, user: User);
    fn player_disconnected(&mut self, user: &str);

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
pub struct PlayerTurn {
    pub token: TurnToken,
    pub state: PlayerGameState,
}

#[derive(PartialEq, Eq, Debug)]
pub struct TurnToken {
    pub user: User,
}

#[derive(PartialEq, Eq, Debug)]
pub enum PlayerMoveResult {
    Ok(PlayerTurn),
    Win,
    Draw,
    InvalidMove(Option<PlayerTurn>),
    InvalidFormat(Option<PlayerTurn>),
}
