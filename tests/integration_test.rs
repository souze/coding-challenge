use async_trait::async_trait;
use code_challenge_game_types::gametraits::{
    GameTrait, PlayerGameState, PlayerMove, PlayerMoveResult, PlayerTurn, TurnToken, User,
};
use futures::channel::oneshot;
use futures::pin_mut;

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;
use tokio::sync::mpsc;

use coding_challenge::controller;

#[derive(Debug)]
struct TestSync<Arg1, RetVal>
where
    Arg1: std::fmt::Debug,
    RetVal: std::fmt::Debug,
{
    channel: mpsc::Receiver<(Arg1, oneshot::Sender<RetVal>)>,
    return_channel: Option<oneshot::Sender<RetVal>>,
}

impl<Arg1, RetVal> TestSync<Arg1, RetVal>
where
    Arg1: std::fmt::Debug,
    RetVal: std::fmt::Debug,
{
    fn expect(&mut self, error_message: impl AsRef<str>) -> Arg1 {
        let fut = self.channel.recv();
        pin_mut!(fut);
        let w = futures::task::noop_waker();
        let mut c = core::task::Context::from_waker(&w);
        let (arg, return_sender) = match fut.as_mut().poll(&mut c) {
            Poll::Ready(Some(val)) => val,
            Poll::Ready(None) => panic!("Player connected channel was dropped"),
            Poll::Pending => panic!("Expected '{}', but didn't get it", error_message.as_ref()),
        };
        self.return_channel = Some(return_sender);
        arg
    }

    fn return_value(&mut self, value: RetVal) {
        self.return_channel.take().unwrap().send(value).unwrap();
    }
}

#[derive(Debug)]
struct MockSync<Arg1, RetVal>
where
    Arg1: std::fmt::Debug,
    RetVal: std::fmt::Debug,
{
    channel: mpsc::Sender<(Arg1, oneshot::Sender<RetVal>)>,
}

impl<Arg1, RetVal> Clone for MockSync<Arg1, RetVal>
where
    Arg1: std::fmt::Debug,
    RetVal: std::fmt::Debug,
{
    fn clone(&self) -> Self {
        Self {
            channel: self.channel.clone(),
        }
    }
}

impl<Arg1, RetVal> MockSync<Arg1, RetVal>
where
    Arg1: std::fmt::Debug,
    RetVal: std::fmt::Debug,
{
    async fn register(&mut self, arg1: Arg1) -> RetVal {
        let (tx, rx) = oneshot::channel();
        self.channel.send((arg1, tx)).await.unwrap();
        rx.await.unwrap()
    }
}

fn make_test_sync<Arg1, RetVal>() -> (TestSync<Arg1, RetVal>, MockSync<Arg1, RetVal>)
where
    Arg1: std::fmt::Debug,
    RetVal: std::fmt::Debug,
{
    let (a, b) = mpsc::channel(1);

    (
        TestSync {
            channel: b,
            return_channel: None,
        },
        MockSync { channel: a },
    )
}

#[derive(Debug, Clone)]
struct MockGame {
    player_connect_sync: MockSync<User, ()>,
    try_start_game_sync: MockSync<(), Option<String>>,
    move_sync: MockSync<(TurnToken, PlayerMove), PlayerMoveResult>,
    reset_sync: MockSync<(), ()>,
}

impl PartialEq for MockGame {
    fn eq(&self, _other: &MockGame) -> bool {
        true
    }
}
impl Eq for MockGame {}

#[async_trait]
impl GameTrait for MockGame {
    async fn player_moves(
        &mut self,
        turn_token: TurnToken,
        player_move: PlayerMove,
    ) -> PlayerMoveResult {
        println!("Controller -> Game: player moves");
        self.move_sync.register((turn_token, player_move)).await
    }

    async fn current_player_disconnected(&mut self, _turn_token: TurnToken) -> Option<PlayerTurn> {
        println!("Controller -> Game: Current player disconnected");
        None
    }

    async fn try_start_game(&mut self) -> Option<PlayerTurn> {
        println!("Controller -> Game: Try to start the game");
        let name = self.try_start_game_sync.register(()).await;
        if let Some(name) = name {
            println!("Game -> Controller: Yup, {name}:s turn");
            Some(PlayerTurn {
                state: PlayerGameState {
                    serialized: "".to_owned(),
                },
                token: TurnToken {
                    user: User {
                        name,
                        color: druid::piet::Color::BLUE,
                    },
                },
            })
        } else {
            println!("Game -> Controller: Nope");
            None
        }
    }

    async fn player_connected(&mut self, user: User) {
        println!("Controller -> Game: Player connected");
        self.player_connect_sync.register(user).await
    }
    async fn player_disconnected(&mut self, _user: &str) {
        println!("Controller -> Game: Player connected");
    }

    async fn reset(&mut self, _users: Vec<User>) {
        println!("Controller -> Game: reset game");
        self.reset_sync.register(()).await;
    }

    fn paint(&self, _ctx: &mut druid::PaintCtx) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn eq(&self, other: &dyn GameTrait) -> bool {
        self == other.as_any().downcast_ref::<MockGame>().unwrap()
    }
}

struct TestGame {
    sut: Option<Sut>,
    player_connect_sync: TestSync<User, ()>,
    try_start_game_sync: TestSync<(), Option<String>>,
    move_sync: TestSync<(TurnToken, PlayerMove), PlayerMoveResult>,
    reset_sync: TestSync<(), ()>,
}

impl TestGame {
    fn poll_sut(&mut self) {
        self.sut.as_mut().unwrap().poll();
    }

    fn expect_player_connected(&mut self, player: &str) {
        let user = self.player_connect_sync.expect("player connected");
        assert_eq!(user.name, player);
        self.player_connect_sync.return_value(());
        self.poll_sut();
    }

    fn expect_try_start_game(&mut self, whos_turn: Option<&str>) {
        self.try_start_game_sync.expect("try start game");
        self.try_start_game_sync
            .return_value(whos_turn.map(|s| s.to_owned()));
        self.poll_sut();
    }

    fn expect_move(
        &mut self,
        expected_player_name: &str,
        expected_move: &str,
        move_result: PlayerMoveResult,
    ) {
        let (turn_token, player_move) = self.move_sync.expect("move");
        assert_eq!(turn_token.user.name, expected_player_name);
        assert_eq!(player_move.serialized, expected_move);
        match &move_result {
            PlayerMoveResult::Ok(pt) => {
                println!(
                    "Game -> Controller: Move OK, next player {}",
                    pt.token.user.name
                )
            }
            PlayerMoveResult::Win => println!("Game -> Controller: WIN"),
            PlayerMoveResult::Draw => println!("Game -> Controller: DRAW"),
            PlayerMoveResult::InvalidMove(_) => println!("Game -> Controller: Invalid move"),
            _ => println!("Game -> Controller: Some result"),
        }
        self.move_sync.return_value(move_result);
        self.poll_sut();
    }

    fn expect_reset(&mut self) {
        self.reset_sync.expect("reset");
        self.reset_sync.return_value(());
        self.poll_sut();
    }
}

fn make_test_game() -> (TestGame, MockGame) {
    let (g, h) = make_test_sync();
    let (i, j) = make_test_sync();
    let (k, l) = make_test_sync();
    let (m, n) = make_test_sync();

    (
        TestGame {
            sut: None,
            player_connect_sync: g,
            try_start_game_sync: i,
            move_sync: k,
            reset_sync: m,
        },
        MockGame {
            player_connect_sync: h,
            try_start_game_sync: j,
            move_sync: l,
            reset_sync: n,
        },
    )
}

async fn sleep_fn(_delay: std::time::Duration) {}

#[derive(Clone)]
struct Sut {
    server_tx: mpsc::Sender<controller::ControllerMsg>,
    sut_fut: Rc<RefCell<Pin<Box<dyn Future<Output = ()>>>>>,
}

impl Sut {
    fn poll(&mut self) {
        let w = futures::task::noop_waker();
        let mut c = core::task::Context::from_waker(&w);
        let _ = self.sut_fut.borrow_mut().as_mut().poll(&mut c);
    }

    fn start() -> (Sut, TestGame) {
        println!("Starting game");
        let (server_tx, server_rx) = tokio::sync::mpsc::channel::<controller::ControllerMsg>(1);
        let (mut test_game, server_game) = make_test_game();
        let boxed_server_game = Box::new(server_game);
        let sut_fut = Box::pin(controller::controller_loop(
            server_rx,
            controller::UiSender::Fake,
            boxed_server_game,
            &sleep_fn,
        ));
        let mut sut = Self {
            server_tx,
            sut_fut: Rc::new(RefCell::new(sut_fut)),
        };
        test_game.sut = Some(sut.clone());

        sut.poll();

        (sut, test_game)
    }

    fn connect_player(&mut self, name: impl AsRef<str>) -> Player {
        println!("Connecting player {:?}", name.as_ref());
        let (tx, rx) = mpsc::channel::<controller::ControllerToPlayerMsg>(15);
        {
            let send_fut = self.server_tx.send(controller::ControllerMsg::ImConnected(
                controller::ImConnectedMsg {
                    player_name: name.as_ref().to_owned(),
                    controller_to_player_sender: tx,
                },
            ));
            pin_mut!(send_fut);
            let w = futures::task::noop_waker();
            let mut c = core::task::Context::from_waker(&w);
            match send_fut.as_mut().poll(&mut c) {
                Poll::Ready(_) => (),
                Poll::Pending => panic!("Server was not ready to receive new connection"),
            }
        }
        self.poll();
        Player {
            name: name.as_ref().to_owned(),
            rx,
            tx: None,
            sut: self.clone(),
        }
    }
}

struct Player {
    name: String,
    rx: mpsc::Receiver<controller::ControllerToPlayerMsg>,
    tx: Option<tokio::sync::oneshot::Sender<controller::PlayerMoveMsg>>,
    sut: Sut,
}

impl Player {
    fn expect_my_turn(&mut self) {
        assert!(self.tx.is_none());
        let fut = self.rx.recv();
        pin_mut!(fut);
        let w = futures::task::noop_waker();
        let mut c = core::task::Context::from_waker(&w);
        let move_sender = match fut.as_mut().poll(&mut c) {
            Poll::Pending => panic!(),
            Poll::Ready(None) => panic!(),
            Poll::Ready(Some(controller::ControllerToPlayerMsg::YourTurn(_, move_sender))) => {
                move_sender
            }
            Poll::Ready(_) => panic!(),
        };
        self.tx = Some(move_sender);
    }

    fn send_move(&mut self, mv: impl AsRef<str>) {
        use code_challenge_game_types::messages::ToClient;
        let (a, _b) = tokio::sync::oneshot::channel::<ToClient>();
        self.tx
            .take()
            .unwrap()
            .send(controller::PlayerMoveMsg {
                mov: PlayerMove {
                    serialized: mv.as_ref().to_owned(),
                },
                move_err_tx: a,
            })
            .unwrap();
        self.sut.poll();
    }
}

fn turn_token(next_player: impl AsRef<str>) -> TurnToken {
    TurnToken {
        user: User {
            name: next_player.as_ref().to_owned(),
            color: druid::piet::Color::BLUE,
        },
    }
}

fn player_turn(player: impl AsRef<str>) -> PlayerTurn {
    PlayerTurn {
        token: turn_token(player.as_ref()),
        state: PlayerGameState {
            serialized: "()".to_owned(),
        },
    }
}

fn ok_move(next_player: impl AsRef<str>, game_state: impl AsRef<str>) -> PlayerMoveResult {
    PlayerMoveResult::Ok(PlayerTurn {
        token: turn_token(next_player),
        state: PlayerGameState {
            serialized: game_state.as_ref().to_owned(),
        },
    })
}

fn connect_n_players(sut: &mut Sut, game: &mut TestGame, n: usize) -> Vec<Player> {
    let mut players = Vec::new();
    for i in 0..n {
        players.push(sut.connect_player(format!("Player{}", i)));
        game.expect_player_connected(&players.last().unwrap().name);
        game.expect_try_start_game(None);
    }
    players
}

#[test]
fn nice_test() {
    env_logger::init();

    let (mut sut, mut game) = Sut::start();

    // Connect Player 1
    let mut p1 = sut.connect_player("Player1");
    game.expect_player_connected(&p1.name);
    game.expect_try_start_game(Some("Player1"));
    p1.expect_my_turn();

    // Player 1 moves
    p1.send_move("");

    // Winning move
    game.expect_move(&p1.name, "", ok_move("Player1", ""));
    p1.expect_my_turn();
    p1.send_move("Some move");
    game.expect_move("Player1", "Some move", PlayerMoveResult::Win);
    game.expect_reset();

    // Game doesn't start
    game.expect_try_start_game(None);

    // Connect Player 2 - game starts, player 2 is first
    let mut p2 = sut.connect_player("Player2");
    game.expect_player_connected("Player2");
    game.expect_try_start_game(Some("Player2"));
    p2.expect_my_turn();

    // Player2 moves
    p2.send_move("hija!");

    // It's a draw
    game.expect_move("Player2", "hija!", PlayerMoveResult::Draw);
    game.expect_reset();
}

#[test]
fn lots_of_players_before_start() {
    let (mut sut, mut game) = Sut::start();

    let mut players = Vec::new();

    // Connect Player 1
    for i in 0..30 {
        players.push(sut.connect_player(format!("Player{}", i)));
        game.expect_player_connected(&players.last().unwrap().name);
        game.expect_try_start_game(None);
    }

    sut.connect_player("LastPlayer");
    game.expect_player_connected("LastPlayer");
    game.expect_try_start_game(Some("Player1"));
    players[1].expect_my_turn();
    players[1].send_move("P1 move");
    game.expect_move("Player1", "P1 move", ok_move("Player25", "game state"));
    players[25].expect_my_turn();
    players[25].send_move("25");
    game.expect_move("Player25", "25", ok_move("Player0", "0"));

    for i in 0..29 {
        players[i].expect_my_turn();
        players[i].send_move("mv");
        game.expect_move(
            &format!("Player{}", i),
            "mv",
            ok_move(format!("Player{}", i + 1), ""),
        )
    }

    players[29].expect_my_turn();
    players[29].send_move("29");
    game.expect_move("Player29", "29", PlayerMoveResult::Win);
    game.expect_reset();
    game.expect_try_start_game(None);
}

#[test]
fn same_player_repeat() {
    let (mut sut, mut game) = Sut::start();

    sut.connect_player("p1");
    game.expect_player_connected("p1");
    game.expect_try_start_game(None);

    let mut p2 = sut.connect_player("p2");
    game.expect_player_connected("p2");
    game.expect_try_start_game(None);

    sut.connect_player("p3");
    game.expect_player_connected("p3");
    game.expect_try_start_game(Some("p2"));

    for _ in 1..5 {
        p2.expect_my_turn();
        p2.send_move("mv");
        game.expect_move("p2", "mv", ok_move("p2", ""));
    }

    p2.expect_my_turn();
    p2.send_move("mv");
    game.expect_move("p2", "mv", PlayerMoveResult::Win);
}

#[test]
fn illegal_move_game_stops() {
    let (mut sut, mut game) = Sut::start();

    let mut players = connect_n_players(&mut sut, &mut game, 5);

    sut.connect_player("last");
    game.expect_player_connected("last");
    game.expect_try_start_game(Some("Player4"));
    players[4].expect_my_turn();
    players[4].send_move("mv");
    game.expect_move("Player4", "mv", PlayerMoveResult::InvalidMove(None));
    game.expect_reset();
}

#[test]
fn illegal_move_game_must_go_on() {
    let (mut sut, mut game) = Sut::start();

    let mut players = connect_n_players(&mut sut, &mut game, 5);

    sut.connect_player("last");
    game.expect_player_connected("last");
    game.expect_try_start_game(Some("Player4"));
    players[4].expect_my_turn();
    players[4].send_move("mv");
    game.expect_move(
        "Player4",
        "mv",
        PlayerMoveResult::InvalidMove(Some(player_turn("Player2"))),
    );
    players[2].expect_my_turn();
    players[2].send_move("2");
    game.expect_move("Player2", "2", PlayerMoveResult::Draw);
}

#[test]
fn player_drops_then_gets_illegal_move_result() {
    let (mut sut, mut game) = Sut::start();
    let mut p1 = sut.connect_player("p1");
    game.expect_player_connected("p1");
    game.expect_try_start_game(Some("p1"));
    p1.expect_my_turn();
    p1.send_move("mv");
    drop(p1);
    game.expect_move("p1", "mv", PlayerMoveResult::InvalidMove(None));
    game.expect_reset();

    let mut p1 = sut.connect_player("p1");
    game.expect_player_connected("p1");
    game.expect_try_start_game(Some("p1"));
    p1.expect_my_turn();
}
