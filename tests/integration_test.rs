use async_trait::async_trait;
use code_challenge_game_types::gametraits::{
    self, GameTrait, PlayerMove, PlayerMoveResult, PlayerTurn, TurnToken, User,
};
use futures::channel::oneshot;
use pin_utils::*;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;

use coding_challenge;
use coding_challenge::controller;

#[derive(Debug, Clone)]
struct MockGame {
    player_moves_channel: mpsc::Sender<(TurnToken, PlayerMove, oneshot::Sender<PlayerMoveResult>)>,
    player_connected_channel: mpsc::Sender<User>,
}

impl PartialEq for MockGame {
    fn eq(&self, other: &MockGame) -> bool {
        true
    }
}
impl Eq for MockGame {}

struct TestGame {}

fn make_test_game() -> (TestGame, MockGame) {
    let (a, b) = mpsc::channel(1);
    let (c, d) = mpsc::channel(1);
    (
        TestGame {},
        MockGame {
            player_moves_channel: a,
            player_connected_channel: c,
        },
    )
}

#[async_trait]
impl GameTrait for MockGame {
    async fn player_moves(
        &mut self,
        turn_token: TurnToken,
        player_move: PlayerMove,
    ) -> PlayerMoveResult {
        let (ret_tx, ret_rx) = oneshot::channel();
        self.player_moves_channel
            .send((turn_token, player_move, ret_tx))
            .await;
        ret_rx.await.unwrap()
    }

    async fn current_player_disconnected(&mut self, turn_token: TurnToken) -> Option<PlayerTurn> {
        None
    }

    async fn try_start_game(&mut self) -> Option<PlayerTurn> {
        None
    }

    async fn player_connected(&mut self, user: User) {
        self.player_connected_channel.send(user).await;
    }
    async fn player_disconnected(&mut self, user: &str) {}

    async fn reset(&mut self, users: Vec<User>) {}

    fn paint(&self, ctx: &mut druid::PaintCtx) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn eq(&self, other: &dyn GameTrait) -> bool {
        self == other.as_any().downcast_ref::<MockGame>().unwrap()
    }
}

struct Sut {
    server_tx: mpsc::Sender<controller::ControllerMsg>,
    sut_fut: Pin<Box<dyn Future<Output = ()>>>,
}

impl Sut {
    fn poll(&mut self) {
        let w = futures::task::noop_waker();
        let mut c = core::task::Context::from_waker(&w);
        let _ = self.sut_fut.as_mut().poll(&mut c);
    }

    fn start() -> (Sut, TestGame) {
        let (server_tx, server_rx) = tokio::sync::mpsc::channel::<controller::ControllerMsg>(1);
        let (test_game, server_game) = make_test_game();
        let boxed_server_game = Box::new(server_game);
        let sut_fut = Box::pin(controller::controller_loop(
            server_rx,
            controller::UiSender::Fake,
            boxed_server_game,
        ));
        let mut sut = Self { server_tx, sut_fut };
        sut.poll();

        (sut, test_game)
    }

    // fn connect_player(&mut self, name: impl AsRef<str>) -> Player {
    //     let (tx, rx) = mpsc::channel::<ToPlayer>(15);
    //     {
    //         let send_fut = self.server_tx.send(ToServer::NewConnection(tx));
    //         pin_mut!(send_fut);
    //         let w = futures::task::noop_waker();
    //         let mut c = core::task::Context::from_waker(&w);
    //         match send_fut.as_mut().poll(&mut c) {
    //             Poll::Ready(_) => (),
    //             Poll::Pending => panic!("Server was not ready to receive new connection"),
    //         }
    //     }
    //     self.poll();
    //     Player {
    //         name: name.as_ref().to_owned(),
    //         rx,
    //         tx: None,
    //     }
    // }
}

#[test]
fn nice_test() {
    assert_eq!(1, 1);

    let (sut, game) = Sut::start();

    // pub type GamePtr = Box<dyn gametraits::GameTrait>;
    // pub type GamePtrMaker = fn(Vec<gametraits::User>) -> GamePtr;
}
