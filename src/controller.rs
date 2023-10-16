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
//         - If we should start a game, send it's your turn to move
//     - PlayerDisconnected
//         - If it's the current player??
//     - Move
//         - Move and check result
//         - Advance player
//     - Setting changed
//         - Update setting
// 3. Update UI
pub async fn controller_loop(
    from_player_rx: &mut mpsc::Receiver<ControllerMsg>,
    ui_sender: UiSender,
    game_maker: GamePtrMaker,
) {
    let mut game = game_maker(vec![]);
    let mut p_move_rx: Option<oneshot::Receiver<PlayerMoveMsg>> = None;
    let mut turn_token: Option<gametraits::TurnToken> = None;
    let mut players = PlayerTable::new();
    let mut controller_info = ControllerInfo::default();
    ui_sender.send_new_state(dyn_clone::clone_box(&*(game)));

    loop {
        let event = if let Some(p_move_rx) = &mut p_move_rx {
            debug!("Waiting for move or control Msg");
            select! {
                v = from_player_rx.recv() => { match v {
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
            match from_player_rx.recv().await {
                Some(msg) => Event::ControllerMsg(msg),
                None => panic!("Connection accept loop dropped its TX"),
            }
        };
        info!("Event: {:?}", event);

        match event {
            Event::ControllerMsg(ControllerMsg::ImConnected(name, tx)) => {
                let new_player = players.add_new_player(name, tx);
                game.player_connected(player_info_to_user(new_player));
                if p_move_rx.is_none() {
                    // The game is not running
                    if let Some(gametraits::PlayerTurn { token, state }) = game.try_start_game() {
                        (p_move_rx, turn_token) = option_tuple_to_tuple_options(
                            your_turn(&mut players, &mut game, token, state, &controller_info)
                                .await,
                        );
                    }
                }
            }
            Event::ControllerMsg(ControllerMsg::ImDisconnected(name)) => {
                if let Some(token) = turn_token {
                    if token.user.name == name {
                        // Current player disconnected
                        if let Some(gametraits::PlayerTurn { token, state }) =
                            game.current_player_disconnected(token)
                        {
                            (p_move_rx, turn_token) = match your_turn(
                                &mut players,
                                &mut game,
                                token,
                                state,
                                &controller_info,
                            )
                            .await
                            {
                                Some((move_rx, token)) => (Some(move_rx), Some(token)),
                                None => (None, None),
                            };
                        } else {
                            // The game has ended because of the disconnect
                            turn_token = None;
                            p_move_rx = None;
                        }
                    } else {
                        // Not the current player disconnected
                        // In some cases, the player might already be out of the game.
                        if players.remove_player(&name) {
                            game.player_disconnected(&name);
                        }
                        turn_token = Some(token);
                    }
                }
            }
            Event::ControllerMsg(ControllerMsg::GoToMode(new_mode)) => {
                let open_gates = matches!(controller_info.game_mode, GameMode::Gating)
                    && !matches!(new_mode, GameMode::Gating);
                controller_info.game_mode = new_mode;
                if open_gates {
                    if let Some(gametraits::PlayerTurn { token, state }) = game.try_start_game() {
                        (p_move_rx, turn_token) = match your_turn(
                            &mut players,
                            &mut game,
                            token,
                            state,
                            &controller_info,
                        )
                        .await
                        {
                            Some((move_rx, token)) => (Some(move_rx), Some(token)),
                            None => (None, None),
                        };
                    }
                }
                if matches!(controller_info.game_mode, GameMode::Gating) {
                    controller_info.reset_scores();
                    game = game_maker(vec![]);

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
                let token = turn_token.unwrap();
                let who_moved = token.user.name.clone();
                let move_result = game.player_moves(token, player_move.mov);
                ui_sender.send_new_state(dyn_clone::clone_box(&*(game)));
                match react_to_player_move(
                    who_moved,
                    move_result,
                    &mut game,
                    &mut controller_info,
                    &mut players,
                    player_move.move_err_tx,
                )
                .await
                {
                    PlayerMovesReturn::None => {
                        debug!("Move result: No more players, or suddenly gating");
                        p_move_rx = None;
                        turn_token = None;
                    }
                    PlayerMovesReturn::NextMoveReceiver(receiver, token) => {
                        debug!(
                            "Move result: keep going, next player: {:?}",
                            token.user.name
                        );
                        p_move_rx = Some(receiver);
                        turn_token = Some(token);
                    }
                    PlayerMovesReturn::GameOver => {
                        debug!("Move result: Game over");
                        tokio::time::sleep(controller_info.windelay).await;
                        game = game_maker(players.iter().map(player_info_to_user).collect());
                        (p_move_rx, turn_token) = option_tuple_to_tuple_options(
                            first_move_new_game(&mut game, &mut controller_info, &mut players)
                                .await,
                        );
                    }
                }
            }
            Event::PlayerMoveDropped => {

                // Do nothing, we'll eventually get an I'm disconnected message

                // if let Some(current_user) = players.current() {
                //     if let Some(unwrapped_turn_token) = turn_token {
                //         if current_user.name == unwrapped_turn_token.user.name {
                //             players.remove_current();
                //             if let Some(gametraits::PlayerTurn { token, state }) =
                //                 game.current_player_disconnected(unwrapped_turn_token)
                //             {
                //                 (p_move_rx, turn_token) = match your_turn(
                //                     &mut players,
                //                     &mut game,
                //                     token,
                //                     state,
                //                     &controller_info,
                //                 )
                //                 .await
                //                 {
                //                     Some((move_rx, token)) => (Some(move_rx), Some(token)),
                //                     None => (None, None),
                //                 };
                //             } else {
                //                 // Game is no longer running because of this disconnect
                //                 turn_token = None;
                //                 p_move_rx = None;
                //             }
                //         }
                //     }
                // }
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
    PlayerMoveDropped,
}

fn option_tuple_to_tuple_options<A, B>(optional_tuple: Option<(A, B)>) -> (Option<A>, Option<B>) {
    match optional_tuple {
        Some((a, b)) => (Some(a), Some(b)),
        None => (None, None),
    }
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

async fn first_move_new_game(
    game: &mut Box<dyn GameTrait>,
    controller_info: &mut ControllerInfo,
    players: &mut PlayerTable,
) -> Option<(oneshot::Receiver<PlayerMoveMsg>, TurnToken)> {
    match game.try_start_game() {
        Some(PlayerTurn { token, state }) => {
            your_turn(players, game, token, state, controller_info).await
        }
        None => None,
    }
}

async fn react_to_player_move(
    who_moved: String,
    player_move_result: PlayerMoveResult,
    game: &mut Box<dyn GameTrait>,
    controller_info: &mut ControllerInfo,
    players: &mut PlayerTable,
    move_err_tx: oneshot::Sender<messages::ToClient>,
) -> PlayerMovesReturn {
    match player_move_result {
        PlayerMoveResult::Ok(PlayerTurn { token, state }) => {
            your_turn(players, game, token, state, controller_info)
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
            move_err_tx.send(messages::INVALID_MOVE).unwrap();
            match maybe_player_turn {
                Some(PlayerTurn { token, state }) => {
                    your_turn(players, game, token, state.clone(), controller_info)
                        .await
                        .into()
                }
                None => PlayerMovesReturn::None,
            }
        }
        PlayerMoveResult::InvalidFormat(maybe_player_turn) => {
            debug!("Invalid move format");
            move_err_tx.send(messages::INVALID_MESSAGE_FORMAT).unwrap();
            match maybe_player_turn {
                Some(PlayerTurn { token, state }) => {
                    your_turn(players, game, token, state.clone(), controller_info)
                        .await
                        .into()
                }
                None => PlayerMovesReturn::None,
            }
        }
    }
}

async fn your_turn(
    players: &mut PlayerTable,
    game: &mut Box<dyn GameTrait>,
    mut turn_token: gametraits::TurnToken,
    mut p_game_state: gametraits::PlayerGameState,
    controller_info: &ControllerInfo,
) -> Option<(oneshot::Receiver<PlayerMoveMsg>, TurnToken)> {
    loop {
        if matches!(controller_info.game_mode, GameMode::Gating) {
            return None;
        }
        tokio::time::sleep(controller_info.turndelay).await;
        let (mov_tx, mov_rx) = oneshot::channel::<PlayerMoveMsg>();
        let new_player = players.get(&turn_token.user.name).unwrap();
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
            match game.current_player_disconnected(turn_token) {
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

#[cfg(test)]
mod test {

    #[test]
    fn i_wish_test() {
        // sut.connect_player("p1");
        // game.expect_try_start(None);
        // sut.connect_player("p2");
        // game.expect_try_start(Some(()));
        // p1.expect_my_move(MakeMove)
        // game.expect_move(this: p1, next: p2)
        // p2.expect_my_move(MakeMove)
        // game.expect_move(this: p2, next: p1)
        // p1.expect_my_move(NoResponse)
        // sut.player_disconnects(p1)
        // game.expect_current_player_disconnected(nextplayer: p2)
        // p2.expect_my_move(MakeMove)
        // game.expect_move(p2, p2)
    }
}
