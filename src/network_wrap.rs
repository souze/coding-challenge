use async_trait::async_trait;
use futures::TryFutureExt;
use log::warn;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufStream;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

pub type SStream = Box<dyn Stream>;

pub async fn bind(addr: &str) -> Result<impl Listener, std::io::Error> {
    TcpListener::bind(addr)
        .map_ok(|inner| RealListener { inner })
        .await
}

#[derive(Debug)]
pub enum NetworkInteraction {
    Sending(String),
    Reading,
}

pub fn get_fake_listener(
    rx: mpsc::Receiver<(
        mpsc::Sender<NetworkInteraction>,
        mpsc::Receiver<String>,
        String,
    )>,
) -> FakeListener {
    FakeListener { rx }
}

#[async_trait]
pub trait Listener {
    async fn accept(&mut self) -> Result<Box<dyn Stream + Send>, Error>;
}

pub struct FakeListener {
    rx: mpsc::Receiver<(
        mpsc::Sender<NetworkInteraction>,
        mpsc::Receiver<String>,
        String,
    )>,
}

#[async_trait]
impl Listener for FakeListener {
    async fn accept(&mut self) -> Result<Box<dyn Stream + Send>, Error> {
        match self.rx.recv().await {
            Some((tx, rx, name)) => Ok(Box::new(FakeStream { tx, rx, name })),
            None => todo!(),
        }
    }
}

pub struct RealListener {
    inner: TcpListener,
}

#[async_trait]
impl Listener for RealListener {
    async fn accept(&mut self) -> Result<Box<dyn Stream + Send>, Error> {
        let a = self
            .inner
            .accept()
            .map_ok(|(inner, _)| {
                Box::new(RealStream {
                    inner: BufStream::new(inner),
                })
            })
            .map_err(|e| e.into())
            .await;

        match a {
            Ok(b) => Ok(b),
            Err(e) => Err(e),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    ConnectionClosed,
    Custom(String),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Custom(error.to_string())
    }
}

#[async_trait]
pub trait Stream {
    async fn read_line(&mut self) -> Result<String, Error>;

    async fn write(&mut self, data: &str) -> Result<(), Error>;
}

pub struct FakeStream {
    tx: mpsc::Sender<NetworkInteraction>,
    rx: mpsc::Receiver<String>,
    name: String,
}

#[async_trait]
impl Stream for FakeStream {
    async fn read_line(&mut self) -> Result<String, Error> {
        println!("Fake stream {} sending reading", self.name);
        match self.tx.send(NetworkInteraction::Reading).await {
            Ok(()) => (),
            Err(_) => return Err(Error::ConnectionClosed),
        }

        println!("Fake stream {} waiting for go", self.name);
        match self.rx.recv().await {
            Some(v) => {
                println!("Test[{}] -> App: {}", self.name, v.trim());
                Ok(v)
            }
            None => Err(Error::ConnectionClosed),
        }
    }

    async fn write(&mut self, data: &str) -> Result<(), Error> {
        println!("App -> Test[{}]: {}", self.name, data.trim());
        match self
            .tx
            .send(NetworkInteraction::Sending(data.to_string()))
            .await
        {
            Ok(()) => (),
            Err(_) => return Err(Error::ConnectionClosed),
        }

        match self.rx.recv().await {
            Some(_) => Ok(()),
            None => Err(Error::ConnectionClosed),
        }
    }
}

pub struct RealStream {
    inner: BufStream<TcpStream>,
}

#[async_trait]
impl Stream for RealStream {
    async fn read_line(&mut self) -> Result<String, Error> {
        let mut line = String::new();
        match self.inner.read_line(&mut line).await {
            Ok(0) => Err(Error::ConnectionClosed),
            Err(_) => Err(Error::Custom("Whoopsie".to_string())),
            Ok(_) => Ok(line),
        }
    }

    async fn write(&mut self, data: &str) -> Result<(), Error> {
        self.inner.write_all(data.as_bytes()).await?;

        self.inner.flush().await?;
        Ok(())
    }
}

pub struct TestDriver {
    // pinbox: Pin<Box<dyn Future<Output = ()>>>,
    // context: Context<'a>,
    new_connection_channel: mpsc::Sender<(
        mpsc::Sender<NetworkInteraction>,
        mpsc::Receiver<String>,
        String,
    )>,
    last_operation: ReadOrSend,
}

enum ReadOrSend {
    Read,
    Send,
}

#[derive(Debug)]
enum ExpectData {
    String(String),
    Anything,
}

impl TestDriver {
    pub fn new(
        // pinbox: Pin<Box<dyn Future<Output = ()>>>,
        // waker: &'a Waker,
        new_connection_channel: mpsc::Sender<(
            mpsc::Sender<NetworkInteraction>,
            mpsc::Receiver<String>,
            String,
        )>,
    ) -> Self {
        Self {
            // pinbox,
            // context: Context::from_waker(&waker),
            new_connection_channel,
            last_operation: ReadOrSend::Read,
        }
    }

    pub async fn connect_user(&mut self, name: &str) -> TestUser {
        // A user is connected
        let (user, app_tx, app_rx) = TestUser::new();
        self.new_connection_channel
            .send((app_tx, app_rx, name.to_string()))
            .await
            .unwrap();

        user
    }

    pub async fn send(&mut self, user: &mut TestUser, data: &str) {
        if matches!(self.last_operation, ReadOrSend::Send) {
            warn!("Sleeping before next send");
            thread::sleep(std::time::Duration::from_millis(100));
        }
        self.last_operation = ReadOrSend::Send;
        match tokio::time::timeout(std::time::Duration::from_millis(100), user.rx.recv()).await {
            Ok(Some(NetworkInteraction::Reading)) => (),
            Ok(Some(NetworkInteraction::Sending(v))) => {
                panic!("Test case wants to send data: {data}; But app is trying to send data: {v}")
            }
            Ok(None) => panic!("Test case wants to send data, but app has disconnected the user"),
            Err(_) => panic!(
                "Timeout waiting for app to expect data from user. Data going to be sent: {data}"
            ),
        }

        user.tx.send(data.to_string() + "\n").await.unwrap();
    }

    async fn internal_receive(&mut self, user: &mut TestUser, expected: ExpectData) {
        self.last_operation = ReadOrSend::Read;
        match tokio::time::timeout(std::time::Duration::from_millis(500), user.rx.recv()).await {
            Ok(Some(NetworkInteraction::Sending(actual_data))) => match expected {
                ExpectData::String(str) => assert_eq!(actual_data, str + "\n"),
                ExpectData::Anything => (),
            },
            Ok(Some(NetworkInteraction::Reading)) => {
                panic!("Test case expects to receive data, but app as also waiting to receive data")
            }
            Ok(None) => panic!(
                "expected user to receive data, instead the user was disconnected by the app"
            ),
            Err(_) => panic!("Timeout waiting to receive {expected:?} from app"),
        }
        user.tx.send("".to_string()).await.unwrap();
    }

    pub async fn receive_anything(&mut self, user: &mut TestUser) {
        self.internal_receive(user, ExpectData::Anything).await;
    }

    pub async fn receive(&mut self, user: &mut TestUser, expected_data: &str) {
        self.internal_receive(user, ExpectData::String(expected_data.to_string()))
            .await;
    }

    pub fn poll(&mut self) {
        // for _ in 0..100 {
        //     let _poll_result: std::task::Poll<()> =
        //         Future::poll(self.pinbox.as_mut(), &mut self.context);
        // }
    }
}

pub struct TestUser {
    tx: mpsc::Sender<String>,
    rx: mpsc::Receiver<NetworkInteraction>,
}

impl TestUser {
    pub fn new() -> (
        Self,
        mpsc::Sender<NetworkInteraction>,
        mpsc::Receiver<String>,
    ) {
        let (app_tx, test_rx) = mpsc::channel::<NetworkInteraction>(1024);
        let (test_tx, app_rx) = mpsc::channel::<String>(1024);

        (
            Self {
                tx: test_tx,
                rx: test_rx,
            },
            app_tx,
            app_rx,
        )
    }
}

// Future test stuff
use std::task::{RawWaker, RawWakerVTable, Waker};
use std::thread;

fn do_nothing(_ptr: *const ()) {}

fn clone(ptr: *const ()) -> RawWaker {
    RawWaker::new(ptr, &VTABLE)
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, do_nothing, do_nothing, do_nothing);

// Future test stuff end

pub fn get_waker() -> Waker {
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

pub type ChannelReportEventTx = mpsc::Sender<NetworkInteraction>;
pub type ChannelReportEventRx = mpsc::Receiver<NetworkInteraction>;
pub type ChannelTestInjectDataTx = mpsc::Sender<String>;
pub type ChannelTestInjectDataRx = mpsc::Receiver<String>;
pub type StreamName = String;
pub type SendNewUserChannel =
    mpsc::Sender<(ChannelReportEventTx, ChannelTestInjectDataRx, StreamName)>;
pub type ReceiveNewUserChannel =
    mpsc::Receiver<(ChannelReportEventTx, ChannelTestInjectDataRx, StreamName)>;

pub fn get_test_channel() -> (SendNewUserChannel, ReceiveNewUserChannel) {
    mpsc::channel::<(ChannelReportEventTx, ChannelTestInjectDataRx, StreamName)>(1024)
}

#[macro_export]
macro_rules! init_flow_test {
    ($driver:ident, $func:ident) => {
        // let $l: u32 = 19;
        let waker = network_wrap::get_waker();
        let (tx, rx) = get_test_channel();
        let fake_listener = network_wrap::get_fake_listener(rx);
        let mut $driver = network_wrap::TestDriver::new(Box::pin($func(fake_listener)), &waker, tx);

        // Poll once to let the app get ready to receive connections
        $driver.poll();
    };
}

#[macro_export]
macro_rules! init_flow_test_spawn {
    ($driver:ident, $func:ident) => {
        let (tx, rx) = get_test_channel();
        let fake_listener = network_wrap::get_fake_listener(rx);
        let mut $driver = network_wrap::TestDriver::new(tx);
        // tokio::spawn(async move { $func(fake_listener).await });
        std::thread::spawn(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                $func(fake_listener).await;
            })
        })
    };
}
