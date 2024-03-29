#![feature(trait_upcasting)]

pub mod async_game_trait;
pub mod controller;
pub mod games;
pub mod network_wrap;
pub mod player_table;
pub mod ui;
pub mod user_connection;

use games::gomoku;

use code_challenge_game_types::gametraits;
use controller::{ControllerMsg, UiSender};
use druid::ExtEventSink;

use log::info;

use async_game_trait::{AsyncGame, AsyncGameTrait};
use tokio::sync::mpsc;

#[tokio::main]
pub async fn main() {
    env_logger::init();
    let listener = network_wrap::bind("127.0.0.1:7654").await.unwrap();

    let controller_channel = mpsc::channel::<ControllerMsg>(1024);
    let async_game = AsyncGame::make_ptr_from_game(gomoku::Game::new(20, 20, Vec::new()));

    let ui_handle = start_ui(controller_channel.0.clone(), async_game.get_paint()).await;

    entry(
        listener,
        UiSender::Real(ui_handle),
        controller_channel,
        async_game,
    )
    .await;
}

async fn start_ui(
    controller_tx: mpsc::Sender<ControllerMsg>,
    game: Box<dyn gametraits::Paint>,
) -> ExtEventSink {
    let (ui_handle_tx, ui_handle_rx) = tokio::sync::oneshot::channel::<ExtEventSink>();
    let cswr = controller::ControllerSender {
        rt_handle: tokio::runtime::Handle::current(),
        tx: controller_tx,
    };
    std::thread::spawn(move || {
        ui::launch(ui_handle_tx, cswr, game);
    });
    info!("Waiting for handle");
    let sink = ui_handle_rx.await.unwrap();
    info!("got handle");
    sink
}

async fn sleep_fn(delay: std::time::Duration) {
    tokio::time::sleep(delay).await;
}

async fn entry(
    listener: impl network_wrap::Listener,
    update_game_sender: UiSender,
    (tx, rx): (mpsc::Sender<ControllerMsg>, mpsc::Receiver<ControllerMsg>),
    actual_game: Box<dyn AsyncGameTrait>,
) {
    tokio::spawn(async move {
        controller::controller_loop(rx, update_game_sender, actual_game, &sleep_fn).await;
    });

    user_connection::accept_connection_loop(listener, tx).await;
}

#[cfg(test)]
mod test {
    use crate::network_wrap::get_test_channel;

    use super::*;

    const JSON_BASIC_STATE: &str = r#"{"your-turn":{"num":0}}"#;

    async fn test_entry(fake_listener: impl network_wrap::Listener) {
        entry(
            fake_listener,
            UiSender::Fake,
            mpsc::channel::<ControllerMsg>(1024),
            AsyncGame::make_ptr_from_game(games::dumb::Game::new()),
        )
        .await;
    }

    async fn test_entry_gomoko(fake_listener: impl network_wrap::Listener) {
        entry(
            fake_listener,
            UiSender::Fake,
            mpsc::channel::<ControllerMsg>(1024),
            AsyncGame::make_ptr_from_game(games::gomoku::Game::new(20, 20, Vec::new())),
        )
        .await;
    }

    async fn test_entry_with_ui(fake_listener: impl network_wrap::Listener) {
        let (tx, rx) = mpsc::channel::<ControllerMsg>(1024);
        let async_game =
            AsyncGame::make_ptr_from_game(games::gomoku::Game::new(20, 20, Vec::new()));
        let sink = start_ui(tx.clone(), async_game.get_paint()).await;
        entry(fake_listener, UiSender::Real(sink), (tx, rx), async_game).await;
    }

    fn login_msg(user: &str, pass: &str) -> String {
        r#"{"auth":{"username":""#.to_string() + user + r#"","password":""# + pass + r#""}}"#
    }

    #[tokio::test]
    async fn test_two_player_flow() {
        init_flow_test_spawn!(driver, test_entry);

        let mut user = driver.connect_user("zeldo").await;
        driver.send(&mut user, &login_msg("zeldo", "pass")).await;

        driver.receive(&mut user, JSON_BASIC_STATE).await;
        driver.send(&mut user, r#"{"move":{"add": 5}}"#).await;

        driver
            .receive(&mut user, r#"{"your-turn":{"num":5}}"#)
            .await;
        driver.send(&mut user, r#"{"move":{"add": 5}}"#).await;

        driver
            .receive(&mut user, r#"{"your-turn":{"num":10}}"#)
            .await;

        let mut user2 = driver.connect_user("user2").await;
        driver.send(&mut user2, &login_msg("user2", "pass")).await;

        driver.send(&mut user, r#"{"move":{"add":3}}"#).await;
        driver
            .receive(&mut user2, r#"{"your-turn":{"num":13}}"#)
            .await;

        driver.send(&mut user2, r#"{"move":{"add":0}}"#).await;

        driver
            .receive(&mut user, r#"{"your-turn":{"num":13}}"#)
            .await;
        driver.send(&mut user, r#"{"move":{"add":3}}"#).await;

        driver
            .receive(&mut user2, r#"{"your-turn":{"num":16}}"#)
            .await;
        driver.send(&mut user2, r#"{"move":{"add":1}}"#).await;

        driver
            .receive(&mut user, r#"{"your-turn":{"num":17}}"#)
            .await;
        driver.send(&mut user, r#"{"move":{"add": 2}}"#).await;

        driver
            .receive(&mut user2, r#"{"your-turn":{"num":19}}"#)
            .await;
    }

    #[tokio::test]
    async fn test_one_player_drops() {
        init_flow_test_spawn!(driver, test_entry);

        let mut user = driver.connect_user("user").await;

        driver.send(&mut user, &login_msg("user", "pass")).await;
        driver
            .receive(&mut user, r#"{"your-turn":{"num":0}}"#)
            .await;

        drop(user);
        driver.poll();
    }

    #[tokio::test]
    async fn invalid_auth() {
        init_flow_test_spawn!(driver, test_entry);

        let mut user = driver.connect_user("user").await;

        driver
            .send(&mut user, r#"{"auth":{"blarh":"user","password":"bleah"}}"#)
            .await;
        driver
            .receive(
                &mut user,
                r#"{"error":{"reason":"invalid message format"}}"#,
            )
            .await;
    }

    #[tokio::test]
    async fn wrong_pass() {
        init_flow_test_spawn!(driver, test_entry);

        let mut user = driver.connect_user("user").await;

        driver.send(&mut user, &login_msg("user", "pass")).await;
        driver
            .receive(&mut user, r#"{"your-turn":{"num":0}}"#)
            .await;

        drop(user);

        let mut user = driver.connect_user("user_connection_2").await;

        driver
            .send(&mut user, &login_msg("user", "wrong pass"))
            .await;
        driver
            .receive(&mut user, r#"{"error":{"reason":"wrong password"}}"#)
            .await;
    }

    #[tokio::test]
    async fn wrong_format_move() {
        init_flow_test_spawn!(driver, test_entry);

        let mut user = driver.connect_user("zeldo").await;
        driver.send(&mut user, &login_msg("user", "pass")).await;

        driver.receive(&mut user, JSON_BASIC_STATE).await;
        driver.send(&mut user, r#"{"move":{"add": 5}}"#).await;

        driver
            .receive(&mut user, r#"{"your-turn":{"num":5}}"#)
            .await;
        driver.send(&mut user, r#"{"sub": 5}"#).await;

        driver
            .receive(
                &mut user,
                r#"{"error":{"reason":"invalid message format"}}"#,
            )
            .await;
    }

    #[tokio::test]
    async fn invalid_move() {
        init_flow_test_spawn!(driver, test_entry_gomoko);

        let mut user = driver.connect_user("zeldo").await;
        driver.send(&mut user, &login_msg("user", "pass")).await;

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":5,"y":5}}"#).await;

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":5,"y":5}}"#).await;

        driver
            .receive(&mut user, r#"{"error":{"reason":"invalid move"}}"#)
            .await;
    }

    #[tokio::test]
    async fn invalid_move_p2() {
        init_flow_test_spawn!(driver, test_entry_gomoko);

        let mut p1 = driver.connect_user("player1").await;
        let mut p2 = driver.connect_user("player2").await;
        driver.send(&mut p1, &login_msg("player1", "pass")).await;
        driver.receive_anything(&mut p1).await;

        driver.send(&mut p2, &login_msg("player2", "pass")).await;

        driver.send(&mut p1, r#"{"move":{"x":5,"y":5}}"#).await;

        driver.receive_anything(&mut p2).await;
        driver.send(&mut p2, r#"{"move":{"x":5,"y":5}}"#).await;

        driver
            .receive(&mut p2, r#"{"error":{"reason":"invalid move"}}"#)
            .await;

        driver.receive_anything(&mut p1).await;
        driver.send(&mut p1, r#"{"move":{"x":5,"y":6}}"#).await;

        driver.receive_anything(&mut p1).await;
    }

    #[tokio::test]
    async fn win_twice() {
        env_logger::init();
        init_flow_test_spawn!(driver, test_entry_gomoko);

        let mut user = driver.connect_user("zeldo").await;
        driver.send(&mut user, &login_msg("zeldo", "pass")).await;

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":0,"y":0}}"#).await;

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":1,"y":0}}"#).await;
        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":2,"y":0}}"#).await;
        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":3,"y":0}}"#).await;
        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":4,"y":0}}"#).await;

        driver
            .receive(&mut user, r#"{"game-over":{"reason":"winner zeldo"}}"#)
            .await;

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":0,"y":0}}"#).await;

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":1,"y":0}}"#).await;
        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":2,"y":0}}"#).await;
        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":3,"y":0}}"#).await;
        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x":4,"y":0}}"#).await;

        driver
            .receive(&mut user, r#"{"game-over":{"reason":"winner zeldo"}}"#)
            .await;
    }

    #[tokio::test]
    async fn two_players_passive_drops() {
        init_flow_test_spawn!(driver, test_entry);

        let mut user = driver.connect_user("user").await;

        driver.send(&mut user, &login_msg("user", "pass")).await;
        driver
            .receive(&mut user, r#"{"your-turn":{"num":0}}"#)
            .await;

        let mut user2 = driver.connect_user("zumba").await;

        driver.send(&mut user2, &login_msg("zumba", "pass")).await;
        drop(user2);

        driver.send(&mut user, r#"{"move":{"add":1}}"#).await;
        driver
            .receive(&mut user, r#"{"your-turn":{"num":1}}"#)
            .await;

        driver.send(&mut user, r#"{"move":{"add":1}}"#).await;
        driver
            .receive(&mut user, r#"{"your-turn":{"num":2}}"#)
            .await;
        driver.send(&mut user, r#"{"move":{"add":1}}"#).await;
        driver
            .receive(&mut user, r#"{"your-turn":{"num":3}}"#)
            .await;
    }

    #[allow(dead_code)]
    fn sleep_a_bit() {
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    #[ignore]
    #[tokio::test]
    async fn test_two_player_flow_with_ui() {
        init_flow_test_spawn!(driver, test_entry_with_ui);

        let mut user = driver.connect_user("zeldo").await;
        driver.send(&mut user, &login_msg("zeldo", "kermit")).await;
        let mut user2 = driver.connect_user("user2").await;
        driver.send(&mut user2, &login_msg("user2", "hello")).await;

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x": 5,"y":7}}"#).await;
        sleep_a_bit();

        driver.receive_anything(&mut user2).await;
        driver.send(&mut user2, r#"{"move":{"x": 1,"y":7}}"#).await;
        sleep_a_bit();

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x": 6,"y":8}}"#).await;
        sleep_a_bit();

        driver.receive_anything(&mut user2).await;
        driver.send(&mut user2, r#"{"move":{"x": 6,"y":7}}"#).await;
        sleep_a_bit();

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x": 7,"y":9}}"#).await;
        sleep_a_bit();

        driver.receive_anything(&mut user2).await;
        driver.send(&mut user2, r#"{"move":{"x": 7,"y":7}}"#).await;
        sleep_a_bit();

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x": 8,"y":10}}"#).await;
        sleep_a_bit();

        driver.receive_anything(&mut user2).await;
        driver.send(&mut user2, r#"{"move":{"x": 8,"y":7}}"#).await;
        sleep_a_bit();

        driver.receive_anything(&mut user).await;
        driver.send(&mut user, r#"{"move":{"x": 9,"y":11}}"#).await;
        sleep_a_bit();
    }
}
