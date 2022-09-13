use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use log::debug;
use tokio::sync::{mpsc, oneshot};

use crate::{
    controller::{ControllerMsg, ControllerToPlayerMsg, GameOverReason, PlayerMoveMsg},
    gametraits,
    messages::{self, Auth, GameOver, ToClient},
    network_wrap,
};

type UserPassDb = Arc<Mutex<HashMap<String, String>>>;

pub(crate) async fn accept_connection_loop(
    mut listener: impl network_wrap::Listener,
    tx: mpsc::Sender<ControllerMsg>,
) {
    let user_password_db: UserPassDb = Arc::new(Mutex::new(HashMap::new()));
    loop {
        debug!("App is waiting for new connections");
        let stream: Box<dyn network_wrap::Stream + Send> = listener.accept().await.unwrap();
        // A new task is spawned for each inbound socket. The socket is
        // moved to the new task and processed there.
        let tx2 = tx.clone();
        let db2 = user_password_db.clone();
        tokio::spawn(async {
            // throw away any error, it's okay, a dropped connection is handled just fine
            match process_user_connection(stream, tx2, db2).await {
                Ok(()) => debug!("User disconnected gracefully"),
                Err(e) => debug!("User disconnected with error {e:?}"),
            }
        });
    }
}

#[derive(Debug)]
enum MyErr {
    AnyHow(String),
}

async fn write_json(stream: &mut Box<dyn network_wrap::Stream + Send>, v: messages::ToClient) {
    stream
        .write(&(serde_json::to_string(&v).unwrap() + "\n"))
        .await
        .unwrap()
}

async fn process_user_connection(
    mut stream: Box<dyn network_wrap::Stream + Send>,
    tx: mpsc::Sender<ControllerMsg>,
    mut user_pass_db: UserPassDb,
) -> Result<(), MyErr> {
    debug!("Got a connection, waiting for auth");
    let (player_game_state_tx, mut from_controller_rx) =
        mpsc::channel::<ControllerToPlayerMsg>(1024);

    // Step 1. Authorize
    let my_name;
    match stream.read_line().await {
        Err(network_wrap::Error::ConnectionClosed) => {
            return Err(MyErr::AnyHow("Closed connection before auth".to_string()));
        }
        Err(_) => {
            return Err(MyErr::AnyHow(
                "Error reading line from connection before auth".to_string(),
            ));
        }
        Ok(line) => {
            match authorize(&line, &mut user_pass_db) {
                Ok(name) => {
                    if tx
                        .send(ControllerMsg::ImConnected(
                            name.clone(),
                            player_game_state_tx,
                        ))
                        .await
                        .is_err()
                    {
                        return Err(MyErr::AnyHow(
                            "Failed sending player connected to controller".to_string(),
                        ));
                    }
                    my_name = name.clone();
                    debug!("Authorization successful");
                    // Send nothing, wait your turn then play!
                }
                Err(response) => {
                    write_json(&mut stream, response).await;
                    return Err(MyErr::AnyHow("Auth failed".to_string()));
                }
            }
        }
    }

    // Step 2. loop -> send state -> get move
    loop {
        // Controller is telling us it's our turn
        debug!("[{my_name}] Waiting for game state from controller");
        let (game_state, move_tx) = match from_controller_rx.recv().await {
            Some(ControllerToPlayerMsg::YourTurn(s, move_tx)) => (s, move_tx),
            Some(ControllerToPlayerMsg::GameOver(reason)) => {
                let reason_str = match reason {
                    GameOverReason::Winner(winner) => "winner ".to_string() + &winner,
                    GameOverReason::Draw => "draw".to_string(),
                };
                write_json(
                    &mut stream,
                    ToClient::GameOver(GameOver { reason: reason_str }),
                )
                .await;
                continue;
            }
            None => return Err(MyErr::AnyHow("Controlled dropped me".to_string())),
        };

        // Send game state to player
        debug!("[{my_name}] Got game state from controller, sending to network user");
        match stream.write(&game_state.serialized).await {
            Ok(()) => (),
            Err(_) => {
                tx.send(ControllerMsg::ImDisconnected(my_name.clone()))
                    .await
                    .unwrap();
                return Err(MyErr::AnyHow("Player disconnected".to_string()));
            }
        }

        // Receive move from player
        debug!("[{my_name}] Game state sent, waiting for network reply from user");
        let player_resp = match stream.read_line().await {
            Err(_) => {
                tx.send(ControllerMsg::ImDisconnected(my_name))
                    .await
                    .unwrap();
                return Err(MyErr::AnyHow(
                    "Error reading line from connection".to_string(),
                ));
            }
            Ok(line) => Ok(line),
        }?;
        debug!("[{my_name}] Got reply from network user");

        let player_move = match player_response_to_move(player_resp.trim()) {
            Ok(p_move) => p_move,
            Err(e) => return Err(e),
        };

        // Send player move to controller
        let (move_err_tx, move_err_rx) = oneshot::channel::<messages::ToClient>();
        match move_tx.send(PlayerMoveMsg {
            mov: player_move,
            move_err_tx,
        }) {
            Ok(_) => {
                if let Ok(err) = move_err_rx.await {
                    write_json(&mut stream, err).await;
                    return Err(MyErr::AnyHow("Move failure".to_string()));
                }
            }
            Err(_) => (), // My move was dropped, whatever
        }
    }
}

#[allow(dead_code)]
fn json_error(err: &str) -> String {
    "{'error': '".to_string() + err + "'}"
}

type Username = String;

fn authorize(line: &str, user_pass_db: &mut UserPassDb) -> Result<Username, ToClient> {
    match serde_json::from_str::<messages::FromClient>(line) {
        Ok(messages::FromClient::Auth(Auth { username, password })) => {
            let db_password = user_pass_db
                .lock()
                .unwrap()
                .get(&username)
                .map(|v| v.to_string());

            match db_password {
                Some(db_password) => {
                    if db_password == password {
                        Ok(username)
                    } else {
                        Err(messages::WRONG_PASSWORD)
                    }
                }
                None => {
                    user_pass_db
                        .lock()
                        .unwrap()
                        .insert(username.clone(), password);
                    Ok(username)
                }
            }
        }
        _ => Err(messages::INVALID_MESSAGE_FORMAT),
    }
}

fn player_response_to_move(line: &str) -> Result<gametraits::PlayerMove, MyErr> {
    Ok(gametraits::PlayerMove {
        serialized: line.to_string(),
    })
}
