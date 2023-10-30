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

    fn paint(&self, ctx: &mut druid::PaintCtx);

    fn as_any(&self) -> &dyn Any;
    fn eq(&self, other: &dyn GameTrait) -> bool;

    fn get_inner(&self) -> Box<dyn GameTrait>;
}

#[derive(Debug, Clone)]
pub struct AsyncGame<T> {
    pub game: T,
}

impl<T> AsyncGame<T>
where
    T: 'static + GameTrait + Clone,
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
    T: 'static + GameTrait + Clone,
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

    fn paint(&self, ctx: &mut druid::PaintCtx) {
        self.game.paint(ctx)
    }

    fn as_any(&self) -> &dyn Any {
        self.game.as_any()
    }
    fn eq(&self, other: &dyn GameTrait) -> bool {
        self.game.eq(other)
    }

    fn get_inner(&self) -> Box<dyn GameTrait> {
        let b = self.game.clone();
        let c = Box::new(b);
        let d = &*c;
        let e = dyn_clone::clone_box(d);
        e
    }
}
