use clap::Parser;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufStream;
use tokio::net::TcpStream;

#[derive(Parser)]
struct Args {
    username: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Hello, world!");

    let args = Args::parse();

    let mut stream = BufStream::new(TcpStream::connect("192.168.25.176:7654").await?);

    let auth = serde_json::to_string(&ToServer::Auth(Auth {
        username: args.username.unwrap_or("bot".to_string()),
        password: "kermit".to_string(),
    }))
    .unwrap()
        + "\n";

    println!("Sending: {}", auth);
    stream.write_all(auth.as_bytes()).await?;
    stream.flush().await?;

    while wait_and_send(&mut stream).await.is_ok() {}

    Ok(())
}

async fn wait_and_send(stream: &mut BufStream<TcpStream>) -> Result<(), std::io::Error> {
    let mut line = String::new();
    stream.read_line(&mut line).await?;
    println!("got: {}", line);

    let from_server = serde_json::from_str::<FromServer>(&line).unwrap();

    let state = match from_server {
        FromServer::YourTurn(state) => state,
        FromServer::GameOver(_) => return Ok(()),
    };

    if let Some(((x, y), _)) = std::iter::zip(
        (0..state.width * state.height).map(|n| (n / state.width, n % state.width)),
        state.cells.iter(),
    )
    .find(|(_, c)| c.is_empty())
    {
        let mov = ToServer::Move(Move { x: y, y: x });
        let mov = serde_json::to_string(&mov)? + "\n";
        println!("Sending: {}", mov);
        stream.write_all(mov.as_bytes()).await?;
        stream.flush().await?;
    } else {
        panic!("No available moves");
    }

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum ToServer {
    Auth(Auth),
    Move(Move),
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct Move {
    x: usize,
    y: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct Auth {
    username: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum FromServer {
    YourTurn(GameState),
    GameOver(GameOver),
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct GameOver {
    #[allow(dead_code)]
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct GameState {
    cells: Vec<Cell>,
    width: usize,
    height: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Cell {
    Empty,
    Occupied(String),
}

impl Cell {
    fn is_empty(&self) -> bool {
        matches!(self, Cell::Empty)
    }
}
