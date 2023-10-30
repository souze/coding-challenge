use async_trait::async_trait;
use code_challenge_game_types::gametraits::*;
use dyn_clone;
use std::{any::Any, fmt::Debug};

#[async_trait]
pub trait AsyncGameTrait: dyn_clone::DynClone + Send + Debug {
    async fn player_moves(
        &mut self,
        turn_token: TurnToken,
        player_move: PlayerMove,
    ) -> PlayerMoveResult;
    async fn current_player_disconnected(&mut self, turn_token: TurnToken) -> Option<PlayerTurn>;

    async fn try_start_game(&mut self) -> Option<PlayerTurn>;

    async fn player_connected(&mut self, user: User);
    async fn player_disconnected(&mut self, user: &str);

    async fn reset(&mut self, users: Vec<User>);

    fn get_paint(&self) -> Box<dyn Paint>;
}

#[derive(Debug, Clone)]
pub struct AsyncGame<T> {
    pub game: T,
}

impl<T> AsyncGame<T>
where
    T: 'static + GameTrait + Clone + Paint,
{
    pub fn new(game: T) -> Self {
        Self { game }
    }

    pub fn make_ptr_from_game(game: T) -> Box<dyn AsyncGameTrait> {
        Box::new(AsyncGame::new(game))
    }
}

#[async_trait]
impl<T> AsyncGameTrait for AsyncGame<T>
where
    T: 'static + GameTrait + Clone + Paint,
{
    async fn player_moves(
        &mut self,
        turn_token: TurnToken,
        player_move: PlayerMove,
    ) -> PlayerMoveResult {
        self.game.player_moves(turn_token, player_move)
    }
    async fn current_player_disconnected(&mut self, turn_token: TurnToken) -> Option<PlayerTurn> {
        self.game.current_player_disconnected(turn_token)
    }

    async fn try_start_game(&mut self) -> Option<PlayerTurn> {
        self.game.try_start_game()
    }

    async fn player_connected(&mut self, user: User) {
        self.game.player_connected(user)
    }
    async fn player_disconnected(&mut self, user: &str) {
        self.game.player_disconnected(user)
    }

    async fn reset(&mut self, users: Vec<User>) {
        self.game.reset(users)
    }

    fn get_paint(&self) -> Box<dyn Paint> {
        let e = dyn_clone::clone_box(&*Box::new(self.game.clone()));
        e
    }
}
