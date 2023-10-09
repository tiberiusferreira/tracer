use quinn::{
    ClientConfig, Connecting, Connection as QuinnConnection, Endpoint, IdleTimeout, RecvStream,
    SendStream, ServerConfig, TransportConfig,
};
use rustls::RootCertStore;
use std::collections::HashMap;
use std::net::{AddrParseError, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use std::{fs, io};
use thiserror::Error;
use tracing::{debug, instrument, subscriber, trace};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer};

pub fn setup_tracing_console_logging_for_test() {
    let filter = {
        let env_filter = EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|e| {
            let default_filter = "info";
            println!(
                "Missing or invalid RUST_LOG, defaulting to {default_filter}. {:#?}",
                e
            );
            EnvFilter::builder()
                .parse(default_filter)
                .unwrap_or_else(|_| panic!("{default_filter} should work as filter"))
        });
        println!("Using env filter: {}", env_filter);
        env_filter
    };
    let fmt = tracing_subscriber::fmt::layer()
        // for tests ansi if nice
        .with_ansi(true)
        .compact()
        .with_filter(filter);
    let subscriber = tracing_subscriber::Registry::default().with(fmt);
    subscriber::set_global_default(subscriber).unwrap();
}

#[derive(Error, Debug)]
pub enum ConnectionDriverError {
    #[error("{0}")]
    ConfigError(#[from] quinn::ConfigError),
    #[error("{0}")]
    TlsError(#[from] rustls::Error),
    #[error("{0}")]
    BindingError(#[from] io::Error),
    #[error("{0}")]
    InvalidConnectionInformation(#[from] AddrParseError),
    #[error("{0}")]
    ConnectionParameters(#[from] quinn::ConnectError),
    #[error("{0}")]
    Connection(#[from] quinn::ConnectionError),
    #[error("{0}")]
    Read(#[from] quinn::ReadError),
    #[error("{0}")]
    ReadExact(#[from] quinn::ReadExactError),
    #[error("{0}")]
    Write(#[from] quinn::WriteError),
    #[error("ReadingStreamTypeAck")]
    ReadingStreamTypeAck,
    #[error("ConnectionClosedBeforeAccept")]
    ConnectionClosedBeforeAccept,
}

fn server_endpoint(
    bind_address: &str,
    cert_chain: Vec<rustls::Certificate>,
    private_key: rustls::PrivateKey,
    connection_idle_timeout: Duration,
) -> Result<Endpoint, ConnectionDriverError> {
    let mut server_config = ServerConfig::with_single_cert(cert_chain, private_key)?;
    let mut transport = TransportConfig::default();
    transport.max_idle_timeout(Some(
        IdleTimeout::try_from(connection_idle_timeout).map_err(quinn::ConfigError::from)?,
    ));
    server_config.transport_config(Arc::new(transport));
    let endpoint = Endpoint::server(server_config, SocketAddr::from_str(bind_address)?)?;
    Ok(endpoint)
}
fn client_endpoint(
    bind_address: &str,
    cert_chain: Vec<rustls::Certificate>,
    connection_idle_timeout: Duration,
) -> Result<Endpoint, ConnectionDriverError> {
    let mut endpoint = Endpoint::client(SocketAddr::from_str(bind_address)?)?;
    let mut root_certs = RootCertStore::empty();
    for c in cert_chain {
        root_certs.add(&c).unwrap();
    }
    let mut transport = TransportConfig::default();
    transport.max_idle_timeout(Some(
        IdleTimeout::try_from(connection_idle_timeout).map_err(quinn::ConfigError::from)?,
    ));
    let mut cc = ClientConfig::with_root_certificates(root_certs);
    cc.transport_config(Arc::new(transport));
    endpoint.set_default_client_config(cc);
    Ok(endpoint)
}

pub fn generate_cert_helper(valid_for_host_names: Vec<String>) {
    let cert_path = "cert.der";
    let key_path = "key.der";
    trace!("generating self-signed certificate");
    let cert = rcgen::generate_simple_self_signed(valid_for_host_names).unwrap();

    let key = cert.serialize_private_key_der();
    let cert = cert.serialize_der().unwrap();
    fs::write(&cert_path, &cert).unwrap();
    fs::write(&key_path, &key).unwrap();
}

pub fn read_cert_from_fs(cert_path: &str) -> Result<rustls::Certificate, io::Error> {
    let cert = rustls::Certificate(fs::read(cert_path)?);
    trace!("loaded self-signed certificate");
    Ok(cert)
}
pub fn read_private_key_from_fs(key_path: &str) -> Result<rustls::PrivateKey, io::Error> {
    let key = rustls::PrivateKey(fs::read(key_path)?);
    trace!("loaded private key");
    Ok(key)
}

#[derive(Clone)]
pub struct Server {
    endpoint: Endpoint,
}

pub struct ConnectionInProgress {
    connecting: Connecting,
}

impl ConnectionInProgress {
    pub async fn wait_connected(self) -> Result<Connection, ConnectionDriverError> {
        let connection = self.connecting.await?;

        Ok(Connection {
            quinn_connection: connection,
        })
    }
}

impl Server {
    pub fn new(
        bind_address: &str,
        cert_chain: Vec<rustls::Certificate>,
        private_key: rustls::PrivateKey,
        connection_idle_timeout: Duration,
    ) -> Result<Self, ConnectionDriverError> {
        let endpoint = server_endpoint(
            bind_address,
            cert_chain,
            private_key,
            connection_idle_timeout,
        )?;
        Ok(Self { endpoint })
    }
    pub async fn accept_connection(&self) -> Result<ConnectionInProgress, ConnectionDriverError> {
        let connecting = self
            .endpoint
            .accept()
            .await
            .ok_or(ConnectionDriverError::ConnectionClosedBeforeAccept)?;
        Ok(ConnectionInProgress { connecting })
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub struct ConnectionTarget {
    pub address: String,
    pub hostname: String,
}
#[derive(Clone)]
pub struct Client {
    endpoint: Endpoint,
    connection_pool: Arc<RwLock<HashMap<ConnectionTarget, Connection>>>,
}

impl Client {
    #[instrument(level = "debug", skip_all)]
    pub async fn connect_or_get_existing(
        &self,
        address: &str,
        hostname: &str,
    ) -> Result<Connection, ConnectionDriverError> {
        debug!("connecting to address: {} and host: {}", address, hostname);
        let mut w_guard = self.connection_pool.write().unwrap();
        let connection_target = ConnectionTarget {
            address: address.to_string(),
            hostname: hostname.to_string(),
        };
        match w_guard.get(&connection_target) {
            Some(existing) => {
                match existing.quinn_connection.close_reason() {
                    None => {
                        trace!("existing connection and not closed, returning it");
                        return Ok(existing.clone());
                    }
                    Some(close_reason) => {
                        trace!("existing connection but was closed, removing it and connecting again: {}", close_reason);
                        w_guard.remove(&connection_target).unwrap();
                    }
                }
            }
            None => {
                trace!("No existing connection, connecting");
            }
        }

        let connection = self
            .endpoint
            .connect(SocketAddr::from_str(address)?, hostname)?
            .await?;
        let connection = Connection {
            quinn_connection: connection,
        };
        w_guard.insert(connection_target, connection.clone());
        Ok(connection)
    }
    pub fn new(
        bind_address: &str,
        cert_chain: Vec<rustls::Certificate>,
        connection_idle_timeout: Duration,
    ) -> Result<Self, ConnectionDriverError> {
        let endpoint = client_endpoint(bind_address, cert_chain, connection_idle_timeout)?;
        Ok(Self {
            endpoint,
            connection_pool: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

#[derive(Clone)]
pub struct Connection {
    quinn_connection: QuinnConnection,
}

impl Connection {
    #[instrument(level = "trace", skip_all)]
    pub async fn start_msg_stream(
        &self,
    ) -> Result<(MessageSender, MessageReceiver), ConnectionDriverError> {
        let (sender, receiver) = self.quinn_connection.open_bi().await?;
        Ok((MessageSender { sender }, MessageReceiver { receiver }))
    }
    #[instrument(level = "trace", skip_all)]
    pub async fn accept_msg_stream(
        &self,
    ) -> Result<(MessageSender, MessageReceiver), ConnectionDriverError> {
        let (sender, receiver) = self.quinn_connection.accept_bi().await?;

        Ok((MessageSender { sender }, MessageReceiver { receiver }))
    }
}

/// Encodes the payload in a length prefixed way, uses fixed 64 bits to encode the length
/// The length is the size in bytes of the payload itself, not account the 64 bits of the length.
#[instrument(level = "trace", skip_all)]
async fn read_length_prefixed_message(
    receiver: &mut RecvStream,
) -> Result<Option<Vec<u8>>, ConnectionDriverError> {
    let mut len: [u8; 8] = [0; 8];
    if let Err(e) = receiver.read_exact(&mut len).await {
        return match e {
            quinn::ReadExactError::FinishedEarly => Ok(None),
            quinn::ReadExactError::ReadError(e) => Err(e.into()),
        };
    }
    let len = u64::from_le_bytes(len);
    trace!("New message of size: {} bytes, reading message", len);
    let len = usize::try_from(len).expect("usize to fit the payload length");
    let mut message: Vec<u8> = vec![0u8; len];
    receiver.read_exact(&mut message).await?;
    Ok(Some(message))
}

#[instrument(level = "trace", skip_all)]
async fn write_length_prefixed_message(
    sender: &mut SendStream,
    message: &[u8],
) -> Result<(), ConnectionDriverError> {
    trace!("Encoding message length, size {} bytes", message.len());
    let len: [u8; 8] = u64::try_from(message.len())
        .expect("u64 to fit usize")
        .to_le_bytes();
    trace!("Sending length: {:?}", len);
    sender.write_all(&len).await?;
    trace!("Sending payload: {:?}", message);
    sender.write_all(&message).await?;
    Ok(())
}

pub struct MessageReceiver {
    receiver: RecvStream,
}
pub struct MessageSender {
    sender: SendStream,
}

impl MessageSender {
    pub async fn enqueue_send_message(&mut self, msg: &[u8]) -> Result<(), ConnectionDriverError> {
        write_length_prefixed_message(&mut self.sender, msg).await
    }
    pub async fn wait_all_sent_and_acked(mut self) -> Result<(), ConnectionDriverError> {
        Ok(self.sender.finish().await?)
    }
}
impl MessageReceiver {
    pub async fn receive_message(&mut self) -> Result<Option<Vec<u8>>, ConnectionDriverError> {
        read_length_prefixed_message(&mut self.receiver).await
    }
}

#[tokio::test]
async fn kitchen_sink() {
    use tracing::{info, warn};
    const CONNECTION_TIMEOUT_MS: u64 = 2000;
    const SERVER_NAME: &str = "localhost";
    const SERVER_ADDRESS: &str = "127.0.0.1:4221";
    const CLIENT_ADD: &str = "127.0.0.1:4222";
    std::env::set_var("RUST_LOG", "connection=trace");
    setup_tracing_console_logging_for_test();
    tokio::task::spawn(async {
        let cert = read_cert_from_fs("cert.der").unwrap();
        let private_key = read_private_key_from_fs("key.der").unwrap();
        let server = Server::new(
            SERVER_ADDRESS,
            vec![cert],
            private_key,
            Duration::from_millis(CONNECTION_TIMEOUT_MS),
        )
        .unwrap();
        while let Ok(connection_in_progress) = server.accept_connection().await {
            tokio::spawn(async {
                debug!("New connection in progress, connecting");
                let connection = connection_in_progress.wait_connected().await.unwrap();
                debug!("connected!");
                {
                    // Maybe Server asks client for configuration information
                    let (mut sender, mut reader) = connection.start_msg_stream().await.unwrap();
                    sender
                        .enqueue_send_message("ServerToClient".as_bytes())
                        .await
                        .unwrap();
                    let response = reader.receive_message().await.unwrap();
                    info!(
                        "Got response: {:?}",
                        String::from_utf8(response.unwrap()).unwrap()
                    );
                }
                tokio::spawn(async move {
                    while let Ok((mut sender, mut reader)) = connection.accept_msg_stream().await {
                        trace!("new stream established");
                        loop {
                            match reader.receive_message().await {
                                Ok(Some(msg)) => {
                                    trace!("Got request: {:?}", msg);
                                    if let Err(e) = sender
                                        .enqueue_send_message("ServerResponse".as_bytes())
                                        .await
                                    {
                                        warn!("Error sending message: {:?}", e);
                                    }
                                }
                                Ok(None) => {
                                    trace!("Stream ended");
                                    break;
                                }
                                Err(e) => {
                                    warn!("Error reading message, aborting stream loop: {:?}", e);
                                    break;
                                }
                            }
                        }
                    }
                });
            });
        }
    });
    // client
    let cert = read_cert_from_fs("cert.der").unwrap();

    let endpoint = Client::new(
        CLIENT_ADD,
        vec![cert],
        Duration::from_secs(CONNECTION_TIMEOUT_MS),
    )
    .unwrap();
    let connection = endpoint
        .connect_or_get_existing(SERVER_ADDRESS, SERVER_NAME)
        .await
        .unwrap();

    let (mut sender, mut receiver) = connection.accept_msg_stream().await.unwrap();
    tokio::spawn(async move {
        while let Ok(Some(msg)) = receiver.receive_message().await {
            info!("Got server request: {}", String::from_utf8(msg).unwrap());
            sender
                .enqueue_send_message("ClientToServerResponse".as_bytes())
                .await
                .unwrap();
        }
    });

    let (mut sender, mut receiver) = connection.start_msg_stream().await.unwrap();
    sender
        .enqueue_send_message("ClienttoServerRequest".as_bytes())
        .await
        .unwrap();
    let msg = receiver.receive_message().await.unwrap().unwrap();
    info!("Got server Response: {}", String::from_utf8(msg).unwrap());
}
