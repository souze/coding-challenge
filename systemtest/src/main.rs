use std::time::Duration;

use log::info;

use rand::Rng;
use subprocess::{self, Popen};

fn main() {
    env_logger::init();
    let _server = subprocess::Popen::create(
        &["../target/debug/coding-challenge"],
        subprocess::PopenConfig::default(),
    )
    .unwrap();

    let mut connected_users = Vec::<(User, Popen)>::new();
    let mut not_connected_users = Vec::<User>::new();
    for i in 1..18 {
        not_connected_users.push(User {
            name: "User".to_string() + &i.to_string(),
            pass: "aoeu".to_string(),
        })
    }

    let mut rng = rand::thread_rng();
    loop {
        let sleep_time: u64 = rng.gen_range(0..500);
        std::thread::sleep(Duration::from_millis(sleep_time));

        info!("doing something!");
        let i = rng.gen_range(0..(not_connected_users.len() + connected_users.len()));

        if i < not_connected_users.len() {
            let user = not_connected_users.remove(i);
            let client = subprocess::Popen::create(
                &["../clients/rust/target/debug/rust", &user.name],
                subprocess::PopenConfig::default(),
            )
            .unwrap();
            connected_users.push((user, client));
        } else {
            let (user, mut client) = connected_users.remove(i - not_connected_users.len());
            info!("Disconnecting {user:?}");
            client.kill().unwrap();
            not_connected_users.push(user);
        }
    }
}

#[derive(Debug)]
struct User {
    name: String,
    pass: String,
}
