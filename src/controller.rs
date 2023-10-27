use std::{collections::HashMap, time::Duration};

use code_challenge_game_types::{
    gametraits::{self, GameTrait, PlayerMoveResult, PlayerTurn, TurnToken, User},
    messages::{self, ToClient},
};
use druid::ExtEventSink;
use log::{debug, info};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};

use crate::{
    player_table::{PlayerInfo, PlayerTable},
    ui,
};

pub type GamePtr = Box<dyn gametraits::GameTrait>;
pub type GamePtrMaker = fn(Vec<gametraits::User>) -> GamePtr;

pub type ErrorSender = oneshot::Sender<ToClient>;

#[derive(Debug)]
pub enum ControllerMsg {
    ImConnected(ImConnectedMsg),
    ImDisconnected(String),
    GoToMode(GameMode),
    ResetGame,
    SetTurnDelay(Duration),
    SetWinDelay(Duration),
}

pub struct ImConnectedMsg {
    pub player_name: String,
    pub controller_to_player_sender: mpsc::Sender<ControllerToPlayerMsg>,
}

impl std::fmt::Debug for ImConnectedMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "'{}'", self.player_name)
    }
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
        color: info.color,
    }
}

// struct GameRunningData {
//     move_receiver: oneshot::Receiver<PlayerMoveMsg>,
//     token: TurnToken,
// }

// impl GameRunningData {
//     fn to_tuple(self) -> (oneshot::Receiver<PlayerMoveMsg>, TurnToken) {
//         (self.move_receiver, self.token)
//     }
// }

// fn run_data_to_some_tuple(run_data: GameRunningData) -> (Option<oneshot::Receiver<PlayerMoveMsg>>, Option<TurnToken>) {

// }

// 1. Wait for any message.
//      - If there are players playing, wait for move
// 2. Perform function based on message
//      - PlayerConnected
//         - If we should start a game, send it's your turn to move
//     - PlayerDisconnected
//         - If it's the current player??
//     - Move
//         - Move and check result
//         - Advance player
//     - Setting changed
//         - Update setting
// 3. Update UI
pub async fn controller_loop<Fut>(
    mut controller_rx: mpsc::Receiver<ControllerMsg>,
    ui_sender: UiSender,
    mut game: Box<dyn GameTrait>,
    sleep_fn: &impl Fn(std::time::Duration) -> Fut,
) where
    Fut: std::future::Future<Output = ()>,
{
    let mut game_running_data: Option<(oneshot::Receiver<PlayerMoveMsg>, TurnToken)> = None;
    let mut players = PlayerTable::new();
    let mut controller_info = ControllerInfo::default();
    ui_sender.send_new_state(dyn_clone::clone_box(&*(game)));

    loop {
        let event = if let Some(p_move_rx) = game_running_data.as_mut().map(|(recv, _)| recv) {
            debug!("Waiting for move or control Msg");
            select! {
                v = controller_rx.recv() => { match v {
                    Some(msg) => Event::ControllerMsg(msg),
                    None => panic!("Connection accept loop dropped its TX"),
                }}
                mov_msg = p_move_rx => { match mov_msg {
                    Ok(msg) => Event::Move(msg),
                    Err(_) => Event::PlayerMoveDropped,
                }}
            }
        } else {
            debug!("Waiting for Control Msg");
            match controller_rx.recv().await {
                Some(msg) => Event::ControllerMsg(msg),
                None => panic!("Connection accept loop dropped its TX"),
            }
        };
        info!("Event: {:?}", event);

        match event {
            Event::ControllerMsg(ControllerMsg::ImConnected(ImConnectedMsg {
                player_name,
                controller_to_player_sender,
            })) => {
                let new_player = players.add_new_player(player_name, controller_to_player_sender);
                game.player_connected(player_info_to_user(new_player)).await;
                if game_running_data.is_none() {
                    // The game is not running
                    if let Some(gametraits::PlayerTurn { token, state }) =
                        game.try_start_game().await
                    {
                        game_running_data = your_turn(
                            &mut players,
                            &mut game,
                            token,
                            state,
                            &controller_info,
                            &sleep_fn,
                        )
                        .await;
                    }
                }
            }
            Event::ControllerMsg(ControllerMsg::ImDisconnected(name)) => {
                if let Some((p_move_rx_2, token)) = game_running_data {
                    if token.user.name == name {
                        // Current player disconnected
                        if let Some(gametraits::PlayerTurn {
                            token: new_token,
                            state,
                        }) = game.current_player_disconnected(token).await
                        {
                            game_running_data = your_turn(
                                &mut players,
                                &mut game,
                                new_token,
                                state,
                                &controller_info,
                                &sleep_fn,
                            )
                            .await;
                        } else {
                            // The game has ended because of the disconnect
                            game_running_data = None;
                        }
                    } else {
                        // Not the current player disconnected
                        // In some cases, the player might already be out of the game.
                        if players.remove_player(&name) {
                            game.player_disconnected(&name).await;
                        }
                        game_running_data = Some((p_move_rx_2, token));
                    }
                }
            }
            Event::ControllerMsg(ControllerMsg::GoToMode(new_mode)) => {
                let open_gates = matches!(controller_info.game_mode, GameMode::Gating)
                    && !matches!(new_mode, GameMode::Gating);
                controller_info.game_mode = new_mode;
                if open_gates {
                    if let Some(gametraits::PlayerTurn { token, state }) =
                        game.try_start_game().await
                    {
                        game_running_data = your_turn(
                            &mut players,
                            &mut game,
                            token,
                            state,
                            &controller_info,
                            &sleep_fn,
                        )
                        .await;
                    }
                }
                if matches!(controller_info.game_mode, GameMode::Gating) {
                    controller_info.reset_scores();
                    game.reset(players.iter().map(player_info_to_user).collect())
                        .await;

                    // Drop any incoming moves
                    game_running_data = None;
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
                let (_, token) = game_running_data.unwrap();
                let who_moved = token.user.name.clone();
                let move_result = game.player_moves(token, player_move.mov).await;
                ui_sender.send_new_state(dyn_clone::clone_box(&*(game)));
                match react_to_player_move(
                    who_moved,
                    move_result,
                    &mut game,
                    &mut controller_info,
                    &mut players,
                    player_move.move_err_tx,
                    &sleep_fn,
                )
                .await
                {
                    PlayerMovesReturn::None => {
                        debug!("Move result: Game is over, probably too few players, after someone quit/got thrown out");
                        game_running_data = None;
                        sleep_fn(controller_info.windelay).await;
                        game.reset(players.iter().map(player_info_to_user).collect())
                            .await;
                    }
                    PlayerMovesReturn::NextMoveReceiver(next_receiver, next_token) => {
                        debug!(
                            "Move result: keep going, next player: {:?}",
                            next_token.user.name
                        );
                        game_running_data = Some((next_receiver, next_token));
                    }
                    PlayerMovesReturn::GameOver => {
                        debug!("Move result: Game over");
                        sleep_fn(controller_info.windelay).await;
                        game.reset(players.iter().map(player_info_to_user).collect())
                            .await;
                        game_running_data = first_move_new_game(
                            &mut game,
                            &mut controller_info,
                            &mut players,
                            &sleep_fn,
                        )
                        .await;
                    }
                }
            }
            Event::PlayerMoveDropped => {
                // Do nothing, we'll eventually get an I'm disconnected message
            }
        } // End event match loop
        controller_info.connected_users = players.iter().map(player_info_to_user).collect();
        ui_sender.send_controller_info(&controller_info);
    }
}

#[derive(Debug)]
enum Event {
    ControllerMsg(ControllerMsg),
    Move(PlayerMoveMsg),
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
    }
    for name in disconnected_players {
        players.remove_player(&name);
    }
}

async fn announce_winner(winner_name: String, players: &mut PlayerTable) {
    send_to_all(players, GameOverReason::Winner(winner_name)).await;
}

async fn announce_draw(players: &mut PlayerTable) {
    send_to_all(players, GameOverReason::Draw).await;
}

enum PlayerMovesReturn {
    None,
    NextMoveReceiver(oneshot::Receiver<PlayerMoveMsg>, TurnToken),
    GameOver,
}

impl From<Option<(oneshot::Receiver<PlayerMoveMsg>, TurnToken)>> for PlayerMovesReturn {
    fn from(a: Option<(oneshot::Receiver<PlayerMoveMsg>, TurnToken)>) -> Self {
        match a {
            Some((receiver, token)) => PlayerMovesReturn::NextMoveReceiver(receiver, token),
            None => PlayerMovesReturn::None,
        }
    }
}

async fn first_move_new_game<Fut>(
    game: &mut Box<dyn GameTrait>,
    controller_info: &mut ControllerInfo,
    players: &mut PlayerTable,
    sleep_fn: &impl Fn(std::time::Duration) -> Fut,
) -> Option<(oneshot::Receiver<PlayerMoveMsg>, TurnToken)>
where
    Fut: std::future::Future<Output = ()>,
{
    match game.try_start_game().await {
        Some(PlayerTurn { token, state }) => {
            your_turn(players, game, token, state, controller_info, sleep_fn).await
        }
        None => None,
    }
}

async fn react_to_player_move<Fut>(
    who_moved: String,
    player_move_result: PlayerMoveResult,
    game: &mut Box<dyn GameTrait>,
    controller_info: &mut ControllerInfo,
    players: &mut PlayerTable,
    move_err_tx: oneshot::Sender<messages::ToClient>,
    sleep_fn: &impl Fn(std::time::Duration) -> Fut,
) -> PlayerMovesReturn
where
    Fut: std::future::Future<Output = ()>,
{
    match player_move_result {
        PlayerMoveResult::Ok(PlayerTurn { token, state }) => {
            your_turn(players, game, token, state, controller_info, sleep_fn)
                .await
                .into()
        }
        PlayerMoveResult::Draw => {
            debug!("Game over, draw");
            announce_draw(players).await;
            PlayerMovesReturn::GameOver
        }
        PlayerMoveResult::Win => {
            debug!("Game over, win");
            announce_winner(who_moved.clone(), players).await;
            controller_info.add_player_win(&who_moved);
            PlayerMovesReturn::GameOver
        }
        PlayerMoveResult::InvalidMove(maybe_player_turn) => {
            debug!("Invalid move from player");
            let _ = move_err_tx.send(messages::INVALID_MOVE); // Player might have disconnected, doesn't matter
            match maybe_player_turn {
                Some(PlayerTurn { token, state }) => your_turn(
                    players,
                    game,
                    token,
                    state.clone(),
                    controller_info,
                    sleep_fn,
                )
                .await
                .into(),
                None => PlayerMovesReturn::None,
            }
        }
        PlayerMoveResult::InvalidFormat(maybe_player_turn) => {
            debug!("Invalid move format");
            move_err_tx.send(messages::INVALID_MESSAGE_FORMAT).unwrap();
            match maybe_player_turn {
                Some(PlayerTurn { token, state }) => your_turn(
                    players,
                    game,
                    token,
                    state.clone(),
                    controller_info,
                    sleep_fn,
                )
                .await
                .into(),
                None => PlayerMovesReturn::None,
            }
        }
    }
}

async fn your_turn<Fut>(
    players: &mut PlayerTable,
    game: &mut Box<dyn GameTrait>,
    mut turn_token: gametraits::TurnToken,
    mut p_game_state: gametraits::PlayerGameState,
    controller_info: &ControllerInfo,
    sleep_fn: &impl Fn(std::time::Duration) -> Fut,
) -> Option<(oneshot::Receiver<PlayerMoveMsg>, TurnToken)>
where
    Fut: std::future::Future<Output = ()>,
{
    loop {
        if matches!(controller_info.game_mode, GameMode::Gating) {
            return None;
        }
        sleep_fn(controller_info.turndelay).await;
        let (mov_tx, mov_rx) = oneshot::channel::<PlayerMoveMsg>();
        let new_player = players.get(&turn_token.user.name).unwrap();
        debug!("Sending 'your turn' to {}", new_player.name);
        if new_player
            .tx
            .send(ControllerToPlayerMsg::YourTurn(
                p_game_state.clone(),
                mov_tx,
            ))
            .await
            .is_err()
        {
            // Player network thread dropped their receiver (probably disconnected)
            match game.current_player_disconnected(turn_token).await {
                Some(gametraits::PlayerTurn { token, state }) => {
                    p_game_state = state;
                    turn_token = token;
                }
                None => {
                    return None;
                }
            }
        } else {
            return Some((mov_rx, turn_token));
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

pub struct PlayerMoveMsg {
    pub mov: gametraits::PlayerMove,
    pub move_err_tx: oneshot::Sender<messages::ToClient>,
}

impl std::fmt::Debug for PlayerMoveMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.mov)
    }
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
