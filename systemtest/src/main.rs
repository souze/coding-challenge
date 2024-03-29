use clap::Parser;
use std::time::Duration;

use log::info;

use rand::Rng;
use subprocess::{self, Exec, Popen};

#[derive(Parser)]
struct Args {
    server_cmd: String,
    client_cmds: Vec<String>,
}

fn main() {
    env_logger::init();

    let args = Args::parse();

    let server_log_file = std::fs::File::create("logs/server.txt").unwrap();
    let _server = Exec::cmd(args.server_cmd)
        .stdout(subprocess::Redirection::Merge)
        .stderr(server_log_file)
        .popen()
        .unwrap();

    let mut connected_users = Vec::<(User, Popen)>::new();
    let mut not_connected_users = Vec::<User>::new();
    for i in 1..14 {
        not_connected_users.push(User {
            name: "User".to_string() + &i.to_string(),
            pass: "aoeu".to_string(),
        })
    }

    let mut rng = rand::thread_rng();
    let mut filename_counter = 0;
    loop {
        // Infinite loop
        let sleep_time: u64 = rng.gen_range(0..5000);
        std::thread::sleep(Duration::from_millis(sleep_time));

        info!("doing something!");
        let i = rng.gen_range(0..(not_connected_users.len() + connected_users.len()));

        if i < not_connected_users.len() {
            let user = not_connected_users.remove(i);
            let file = std::fs::File::create(format!(
                "logs/client_out{}.txt",
                filename_counter.to_string()
            ))
            .unwrap();
            filename_counter += 1;
            let client_index = rng.gen_range(0..args.client_cmds.len());
            let client = subprocess::Exec::cmd(&args.client_cmds[client_index])
                .arg(&user.name)
                .arg(&user.pass)
                .arg("127.0.0.1:7654")
                .stdout(subprocess::Redirection::Merge)
                .stderr(subprocess::Redirection::File(file))
                .detached()
                .popen()
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
