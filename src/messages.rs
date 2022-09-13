use serde::{Deserialize, Serialize};

/// Client -> Server

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FromClient {
    Auth(Auth),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Move<T> {
    Move(T),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Auth {
    pub username: String,
    pub password: String,
}

mod test {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn serialized_str() {
        assert_eq!(
            serde_json::to_string(&FromClient::Auth(Auth {
                username: "user".to_string(),
                password: "pass".to_string(),
            }))
            .unwrap(),
            r#"{"auth":{"username":"user","password":"pass"}}"#.to_string()
        );
    }
}

/// To Client

#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum ToClient {
    Error(Error),
    GameOver(GameOver),
}

#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct GameOver {
    pub reason: String,
}

#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum YourTurn<State> {
    YourTurn(State),
}

#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
pub struct Error {
    pub reason: &'static str,
}

pub const INVALID_MESSAGE_FORMAT: ToClient = ToClient::Error(Error {
    reason: "invalid message format",
});
pub const WRONG_PASSWORD: ToClient = ToClient::Error(Error {
    reason: "wrong password",
});
pub const INVALID_MOVE: ToClient = ToClient::Error(Error {
    reason: "invalid move",
});
