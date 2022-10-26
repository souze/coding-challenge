use std::{collections::HashMap, time::Duration};

use druid::ExtEventSink;
use log::{debug, info};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};

use crate::{
    gametraits::{self, GameTrait, PlayerMoveResult, User},
    messages::{self, ToClient},
    player_table::{PlayerInfo, PlayerTable},
    ui,
};

pub type GamePtr = Box<dyn gametraits::GameTrait>;
pub type GamePtrMaker = fn() -> GamePtr;

pub type ErrorSender = oneshot::Sender<ToClient>;

#[derive(Debug)]
pub enum ControllerMsg {
    ImConnected(String, mpsc::Sender<ControllerToPlayerMsg>),
    ImDisconnected(String),
    GoToMode(GameMode),
    ResetGame,
    SetTurnDelay(Duration),
    SetWinDelay(Duration),
}

#[derive(Clone)]
pub struct ControllerSender {
    pub rt_handle: tokio::runtime::Handle,
    pub tx: mpsc::Sender<ControllerMsg>,
}

#[derive(Clone)]
pub struct ControllerInfo {
    pub connected_users: Vec<User>,
    pub game_mode: GameMode,
    pub score: HashMap<String, u64>,
    pub turndelay: Duration,
    pub windelay: Duration,
}

impl Default for ControllerInfo {
    fn default() -> Self {
        Self {
            connected_users: Default::default(),
            game_mode: GameMode::Practice,
            score: HashMap::default(),
            turndelay: Duration::from_millis(200),
            windelay: Duration::from_millis(500),
        }
    }
}

impl ControllerInfo {
    fn add_player_win(&mut self, name: &String) {
        // Hehe
        match self.score.get_mut(name) {
            Some(current_score) => {
                *current_score += 1;
            }
            None => {
                self.score.insert(name.to_string(), 1);
            }
        }
    }

    fn reset_scores(&mut self) {
        self.score = HashMap::new();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameMode {
    Practice,
    Gating,
    Competition,
}

fn player_info_to_user(info: &PlayerInfo) -> User {
    User {
        name: info.name.clone(),
        color: info.color.clone(),
    }
}

// 1. Wait for any message.
//      - If there are players playing, wait for move
// 2. Perform function based on message
//      - PlayerConnected
///         - If we should start a game, send it's your turn to move
///     - PlayerDisconnected
///         - If it's the current player??
///     - Move
///         - Move and check result
///         - Advance player
///     - Setting changed
///         - Update setting
/// 3. Update UI
pub async fn controller_loop(
    from_player_rx: &mut mpsc::Receiver<ControllerMsg>,
    ui_sender: UiSender,
    game_maker: GamePtrMaker,
) {
    let mut game = game_maker();
    let mut p_move_rx: Option<oneshot::Receiver<PlayerMoveMsg>> = None;
    // let mut next_p_move_rx: Option<oneshot::Receiver<PlayerMoveMsg>> = None;
    let mut players = PlayerTable::new();
    let mut controller_info = ControllerInfo::default();
    ui_sender.send_new_state(dyn_clone::clone_box(&*game));

    loop {
        let event = if let Some(p_move_rx) = &mut p_move_rx {
            debug!("Waiting for move or control Msg");
            select! {
                v = from_player_rx.recv() => { match v {
                    Some(msg) => Event::ControllerMsg(msg),
                    None => Event::ReceiveReturnedNone,
                }}
                mov_msg = p_move_rx => { match mov_msg {
                    Ok(msg) => Event::Move(msg),
                    Err(_) => Event::PlayerMoveDropped,
                }}
            }
        } else {
            debug!("Waiting for Control Msg");
            match from_player_rx.recv().await {
                Some(msg) => Event::ControllerMsg(msg),
                None => Event::ReceiveReturnedNone,
            }
        };
        info!("Event: {:?}", event);

        match event {
            Event::ControllerMsg(ControllerMsg::ImConnected(name, tx)) => {
                players.add_new_player(name, tx);
                if players.iter().len() == 1 {
                    p_move_rx = your_turn(&mut players, &mut game, &controller_info).await;
                }
            }
            Event::ControllerMsg(ControllerMsg::ImDisconnected(name)) => {
                if players
                    .current()
                    .map_or(false, |player| player.name == name)
                {
                    players.remove_current();
                    p_move_rx = your_turn(&mut players, &mut game, &controller_info).await;
                } else {
                    players.remove_player(&name);
                }
            }
            Event::ControllerMsg(ControllerMsg::GoToMode(new_mode)) => {
                let open_gates = matches!(controller_info.game_mode, GameMode::Gating)
                    && !matches!(new_mode, GameMode::Gating);
                controller_info.game_mode = new_mode;
                if open_gates {
                    p_move_rx = your_turn(&mut players, &mut game, &controller_info).await;
                }
                if matches!(controller_info.game_mode, GameMode::Gating) {
                    controller_info.reset_scores();
                    game.reset();
                    players.shuffle();

                    // Drop any incoming moves
                    p_move_rx = None;
                }
            }
            Event::ControllerMsg(ControllerMsg::ResetGame) => {
                // TODO?
            }
            Event::ControllerMsg(ControllerMsg::SetTurnDelay(delay)) => {
                controller_info.turndelay = delay
            }
            Event::ControllerMsg(ControllerMsg::SetWinDelay(delay)) => {
                controller_info.windelay = delay
            }
            Event::Move(player_move) => {
                let player = &player_info_to_user(players.current().unwrap());

                let move_result = game.player_moves(player, player_move.mov);
                ui_sender.send_new_state(dyn_clone::clone_box(&*game));
                match react_to_player_move(
                    move_result,
                    &mut game,
                    &mut controller_info,
                    &mut players,
                    player_move.move_err_tx,
                )
                .await
                {
                    PlayerMovesReturn::None => {
                        // No more players, or suddenly gating
                        p_move_rx = None;
                    }
                    PlayerMovesReturn::NextMoveReceiver(receiver) => {
                        p_move_rx = Some(receiver);
                    }
                    PlayerMovesReturn::GameOver => {
                        tokio::time::sleep(controller_info.windelay).await;
                        p_move_rx =
                            first_move_new_game(&mut game, &mut controller_info, &mut players)
                                .await;
                    }
                }
            }
            Event::ReceiveReturnedNone => panic!("Connection acception loop has dropped it's tx"),
            Event::PlayerMoveDropped => {
                players.remove_current();
                p_move_rx = your_turn(&mut players, &mut game, &controller_info).await;
            }
        }
        controller_info.connected_users = players.iter().map(player_info_to_user).collect();
        ui_sender.send_controller_info(&controller_info);
    }
}

#[derive(Debug)]
enum Event {
    ControllerMsg(ControllerMsg),
    Move(PlayerMoveMsg),
    ReceiveReturnedNone,
    PlayerMoveDropped,
}

async fn send_to_all(players: &mut PlayerTable, msg: GameOverReason) {
    let mut disconnected_players = Vec::<String>::new();
    for p in players.iter() {
        if p.tx
            .send(ControllerToPlayerMsg::GameOver(msg.clone()))
            .await
            .is_err()
        {
            disconnected_players.push(p.name.clone());
        }
        // It might be that some players are disconnected here. Whatever, maybe we'll notice later
    }
    for name in disconnected_players {
        players.remove_player(&name);
    }
}

async fn announce_winner(players: &mut PlayerTable) {
    let winner_name = players.current().unwrap().name.clone();
    send_to_all(players, GameOverReason::Winner(winner_name)).await;
}

async fn announce_draw(players: &mut PlayerTable) {
    send_to_all(players, GameOverReason::Draw).await;
}

enum PlayerMovesReturn {
    None,
    NextMoveReceiver(oneshot::Receiver<PlayerMoveMsg>),
    GameOver,
}

impl From<Option<oneshot::Receiver<PlayerMoveMsg>>> for PlayerMovesReturn {
    fn from(a: Option<oneshot::Receiver<PlayerMoveMsg>>) -> Self {
        match a {
            Some(receiver) => PlayerMovesReturn::NextMoveReceiver(receiver),
            None => PlayerMovesReturn::None,
        }
    }
}

async fn first_move_new_game(
    game: &mut Box<dyn GameTrait>,
    controller_info: &mut ControllerInfo,
    players: &mut PlayerTable,
) -> Option<oneshot::Receiver<PlayerMoveMsg>> {
    game.reset();
    players.shuffle();
    your_turn(players, game, controller_info).await
}

async fn react_to_player_move(
    player_move_result: PlayerMoveResult,
    game: &mut Box<dyn GameTrait>,
    controller_info: &mut ControllerInfo,
    players: &mut PlayerTable,
    move_err_tx: oneshot::Sender<messages::ToClient>,
) -> PlayerMovesReturn {
    match player_move_result {
        PlayerMoveResult::Ok => {
            players.advance_player();
            your_turn(players, game, controller_info).await.into()
        }
        PlayerMoveResult::Draw => {
            debug!("Game over, draw");
            announce_draw(players).await;
            PlayerMovesReturn::GameOver
        }
        PlayerMoveResult::Win => {
            debug!("Game over, win");
            announce_winner(players).await;
            controller_info.add_player_win(&players.current().unwrap().name);
            PlayerMovesReturn::GameOver
        }
        PlayerMoveResult::InvalidMove => {
            debug!("Invalid move from player");
            move_err_tx.send(messages::INVALID_MOVE).unwrap();
            players.remove_current();
            your_turn(players, game, controller_info).await.into()
        }
        PlayerMoveResult::InvalidFormat => {
            debug!("Invalid move format");
            move_err_tx.send(messages::INVALID_MESSAGE_FORMAT).unwrap();
            players.remove_current();
            your_turn(players, game, controller_info).await.into()
        }
    }
}

async fn player_moves(
    game: &mut Box<dyn GameTrait>,
    controller_info: &mut ControllerInfo,
    players: &mut PlayerTable,
    PlayerMoveMsg { mov, move_err_tx }: PlayerMoveMsg,
) -> PlayerMovesReturn {
    let player = &player_info_to_user(players.current().unwrap());

    match game.player_moves(player, mov) {
        PlayerMoveResult::Ok => {
            players.advance_player();
            your_turn(players, game, controller_info).await.into()
        }
        PlayerMoveResult::Draw => {
            debug!("Game over, draw");
            announce_draw(players).await;
            PlayerMovesReturn::GameOver
        }
        PlayerMoveResult::Win => {
            debug!("Game over, win");
            announce_winner(players).await;
            controller_info.add_player_win(&players.current().unwrap().name);
            PlayerMovesReturn::GameOver
        }
        PlayerMoveResult::InvalidMove => {
            debug!("Invalid move from player");
            move_err_tx.send(messages::INVALID_MOVE).unwrap();
            players.remove_current();
            your_turn(players, game, controller_info).await.into()
        }
        PlayerMoveResult::InvalidFormat => {
            debug!("Invalid move format");
            move_err_tx.send(messages::INVALID_MESSAGE_FORMAT).unwrap();
            players.remove_current();
            your_turn(players, game, controller_info).await.into()
        }
    }
}

async fn your_turn(
    players: &mut PlayerTable,
    game: &mut Box<dyn GameTrait>,
    controller_info: &ControllerInfo,
) -> Option<oneshot::Receiver<PlayerMoveMsg>> {
    loop {
        if players.is_empty() || matches!(controller_info.game_mode, GameMode::Gating) {
            return None;
        }
        tokio::time::sleep(controller_info.turndelay).await;
        let new_player = players.current().unwrap();
        let (mov_tx, mov_rx) = oneshot::channel::<PlayerMoveMsg>();
        let p_game_state = game.get_player_state(&player_info_to_user(new_player));
        if new_player
            .tx
            .send(ControllerToPlayerMsg::YourTurn(p_game_state, mov_tx))
            .await
            .is_err()
        {
            players.remove_current();
        } else {
            return Some(mov_rx);
        }
    }
}

pub enum UiSender {
    Real(ExtEventSink),
    Fake,
}

impl UiSender {
    fn send_new_state(&self, p_state: Box<dyn gametraits::GameTrait>) {
        debug!("Sending new game state to UI");
        match self {
            UiSender::Real(tx) => Self::real_send_new_state(tx, p_state),
            UiSender::Fake => (),
        }
    }

    fn real_send_new_state(tx: &ExtEventSink, p_state: Box<dyn gametraits::GameTrait>) {
        tx.submit_command(ui::UI_UPDATE_COMMAND, p_state, druid::Target::Global)
            .unwrap();
    }

    fn send_controller_info(&self, controller_info: &ControllerInfo) {
        match self {
            UiSender::Fake => (),
            UiSender::Real(tx) => tx
                .submit_command(
                    ui::UI_UPDATE_CONTROLLER_INFO_COMMAND,
                    controller_info.clone(),
                    druid::Target::Global,
                )
                .unwrap(),
        }
    }
}

#[derive(Debug)]
pub struct PlayerMoveMsg {
    pub mov: gametraits::PlayerMove,
    pub move_err_tx: oneshot::Sender<messages::ToClient>,
}

pub enum ControllerToPlayerMsg {
    YourTurn(gametraits::PlayerGameState, oneshot::Sender<PlayerMoveMsg>),
    GameOver(GameOverReason),
}

#[derive(Clone)]
pub enum GameOverReason {
    Winner(String),
    Draw,
}

impl ControllerSender {
    pub fn send(&self, msg: ControllerMsg) {
        let tx2 = self.tx.clone();
        self.rt_handle.spawn(async move {
            tx2.send(msg).await.unwrap();
        });
    }
}
