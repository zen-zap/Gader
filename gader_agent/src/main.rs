use core::{
    default::Default,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};
use std::sync::Arc;

use anyhow::{Context, Result};
use bollard::{API_DEFAULT_VERSION, Docker, query_parameters::LogsOptionsBuilder};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use gader_agent::{
    AppState, cert, config,
    parsers::{LogParser, immich, vaultwarden},
};
use gader_common::{LogEntry, NetworkPacket};
use quinn::{Endpoint, IdleTimeout, ServerConfig, TransportConfig};
use subtle::ConstantTimeEq;
use tokio::sync::broadcast;
use tokio_util::{
    codec::{FramedRead, FramedWrite, length_delimited::LengthDelimitedCodec},
    sync::CancellationToken,
};
use tracing::{debug, error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let docker_conn = Docker::connect_with_http("http://127.0.0.1:2375", 5, API_DEFAULT_VERSION)
        .expect("Unable to connect to docker");

    let server_endpoint = get_connection_endpoint().context("Error in making endpoint")?;
    info!("got server connection endpoint");

    let secret = config::load_secret().context("Failed to load secret")?;
    let state = Arc::new(AppState::new(150, secret));

    let (tx, _) = broadcast::channel::<LogEntry>(1000);
    let c_token = CancellationToken::new();

    info!("spawning tasks for immich_server and vaultwarden containers");

    let tx_immich = tx.clone();
    let docker_immich = docker_conn.clone();
    let c_im = c_token.clone();
    let state_im = state.clone();
    let _task_immich = tokio::spawn(async move {
        let immich_parser = immich::ImmichParser::new();
        spawn_watcher(
            docker_immich,
            "immich_server",
            immich_parser,
            tx_immich,
            c_im,
            state_im,
        )
        .await;
    });

    let tx_vw = tx.clone();
    let docker_vw = docker_conn.clone();
    let c_vw = c_token.clone();
    let state_vw = state.clone();
    let _task_vw = tokio::spawn(async move {
        let vw_parser = vaultwarden::VWParser::new();
        spawn_watcher(docker_vw, "vaultwarden", vw_parser, tx_vw, c_vw, state_vw).await;
    });

    info!("Awaiting connections");
    loop {
        tokio::select! {
            Some(conn) = server_endpoint.accept() => {
                info!("Accepting a client");
                tokio::spawn(handle_client(conn, tx.clone(), c_token.clone(), state.clone()));
            }
            _ = tokio::signal::ctrl_c() => {
                info!("SIGINT received, cancelling tasks...");
                c_token.cancel();
                break;
            }
        }
    }

    // wait for all QUIC connections to send CONNECTION_CLOSE and drain cleanly
    server_endpoint.wait_idle().await;

    Ok(())
}

fn get_connection_endpoint() -> Result<Endpoint> {
    let (cert_chain, key_der) =
        cert::load_or_generate_keys().context("Failed to load/generate TLS keys")?;

    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)
        .context("failed to build TLS config")?;

    crypto.alpn_protocols = vec![b"gader-v1".to_vec()];

    let quic_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(crypto)?;

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 23456);

    let mut transport_config = TransportConfig::default();
    transport_config.keep_alive_interval(Some(Duration::from_secs(20)));
    transport_config.max_idle_timeout(Some(
        IdleTimeout::try_from(Duration::from_secs(60)).context("Invalid idle timeout")?,
    ));

    let mut quic_server_config = ServerConfig::with_crypto(Arc::new(quic_crypto));

    quic_server_config.transport_config(Arc::new(transport_config));

    let server_endpoint = Endpoint::server(quic_server_config, socket_addr)?;

    Ok(server_endpoint)
}

async fn spawn_watcher<P: LogParser>(
    docker: Docker,
    name: &str,
    parser: P,
    tx: broadcast::Sender<LogEntry>,
    c_token: CancellationToken,
    state: Arc<AppState>,
) {
    info!("Watching: {}", name);

    loop {
        let params = LogsOptionsBuilder::new()
            .follow(true)
            .stderr(true)
            .stdout(true)
            .tail("30")
            .build();

        let mut stream = docker.logs(name, Some(params));

        let should_retry = loop {
            tokio::select! {
                recv = stream.next() => {
                    match recv {
                        Some(Ok(log)) => {
                            debug!("{:?}", log);
                            if let Some(entry) = parser.parse(&log.to_string()) {
                                debug!("Receiving logs!");
                                state.add_log(entry.clone());
                                let _ = tx.send(entry);
                            }
                        }
                        Some(Err(e)) => {
                            error!("Docker stream error for '{}': {}", name, e);
                        }
                        None => {
                            info!("Docker log stream for '{}' ended — will retry in 5s", name);
                            break true;
                        }
                    }
                }
                _ = c_token.cancelled() => {
                    debug!("Watcher '{}' received cancel signal", name);
                    break false;
                }
            }
        };

        if !should_retry {
            break;
        }

        // Wait before reconnecting, but respect cancellation.
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = c_token.cancelled() => {
                debug!("Watcher '{}' cancelled during retry wait", name);
                break;
            }
        }
    }
}

async fn handle_client(
    conn: quinn::Incoming,
    tx: broadcast::Sender<LogEntry>,
    c_token: CancellationToken,
    state: Arc<AppState>,
) {
    let connection = match conn.await {
        Ok(c) => {
            debug!("Connected to a client");
            c
        }
        Err(e) => {
            error!("Connection to client failed: {}", e);
            return;
        }
    };

    info!(
        "Client connected: {} === Starting Handshake",
        connection.remote_address()
    );

    let (send_stream, recv_stream) = match connection.accept_bi().await {
        Ok(s) => {
            info!("Received bi-stream");
            s
        }
        Err(e) => {
            error!("Failed to accept bi-stream: {}", e);
            return;
        }
    };

    let mut writer = FramedWrite::new(send_stream, LengthDelimitedCodec::new());
    let mut reader = FramedRead::new(recv_stream, LengthDelimitedCodec::new());

    let handshake_res = tokio::time::timeout(Duration::from_secs(3), reader.next()).await;

    match handshake_res {
        Ok(Some(Ok(bytes))) => match postcard::from_bytes::<NetworkPacket>(&bytes) {
            Ok(NetworkPacket::Handshake { secret_token }) => {
                if secret_token
                    .as_bytes()
                    .ct_eq(state.secret.as_bytes())
                    .into()
                {
                    info!("Handshake successful for {}", connection.remote_address());
                    let ack = NetworkPacket::HandshakeAck { accepted: true };
                    if let Ok(data) = postcard::to_stdvec(&ack) {
                        let _ = writer.send(Bytes::from(data)).await;
                    }
                } else {
                    info!(
                        "Wrong secret from {}. Aborting connection!",
                        connection.remote_address()
                    );
                    let nack = NetworkPacket::HandshakeAck { accepted: false };
                    if let Ok(data) = postcard::to_stdvec(&nack) {
                        let _ = writer.send(Bytes::from(data)).await;
                    }
                    return;
                }
            }
            _ => {
                info!(
                    "Invalid handshake packet from {}. Aborting connection!",
                    connection.remote_address()
                );
                let nack = NetworkPacket::HandshakeAck { accepted: false };
                if let Ok(data) = postcard::to_stdvec(&nack) {
                    let _ = writer.send(Bytes::from(data)).await;
                }
                return;
            }
        },
        _ => {
            error!(
                "Handshake timed out for client: {}",
                connection.remote_address()
            );
            return;
        }
    }

    let mut rx = tx.subscribe();
    let mut batch: Vec<LogEntry> = Vec::with_capacity(10);

    let mut filter: Option<String> = None;
    let mut flush_timer = tokio::time::interval(tokio::time::Duration::from_millis(500));

    let history = state.get_snapshot();
    if !history.is_empty() {
        let packet = NetworkPacket::Batch(history);
        if let Ok(data) = postcard::to_stdvec(&packet) {
            writer.send(Bytes::from(data)).await.ok();
        }
    }

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(entry) => {
                        if let Some(ref svc) = filter
                            && !entry.service.eq_ignore_ascii_case(svc) {
                                continue;
                            }

                        batch.push(entry);

                        if batch.len() >= 10 {
                            let batch_to_send = std::mem::take(&mut batch);
                            let packet = NetworkPacket::Batch(batch_to_send);

                            debug!("Sending packet: {:#?}", packet);

                            if let Ok(data) = postcard::to_stdvec(&packet)
                                && writer.send(Bytes::from(data)).await.is_err() {
                                    break;
                                }
                        }
                    }
                    Err(e) => {
                        error!("Encountered Error: {:?}", e);
                    }
                }
            }

            packet_res = reader.next() => {
                match packet_res {
                    Some(Ok(bytes)) => {
                        if let Ok(packet) = postcard::from_bytes::<NetworkPacket>(&bytes)
                            && let NetworkPacket::UpdateFilter { service } = packet {
                                info!("Updating filter to: {:?}", service);
                                filter = service;
                                batch.clear();
                            }
                    }
                    Some(Err(e)) => {
                        error!("Framing Error: {}", e);
                        break;
                    }
                    None => break,
                }
            }

            _ = c_token.cancelled() => {
                info!("Client handler shutting down -- received cancel signal");
                break;
            }

            _ = flush_timer.tick() => {

                if !batch.is_empty() {

                    let batch_to_send = std::mem::take(&mut batch);
                    let packet = NetworkPacket::Batch(batch_to_send);
                    debug!("Sending packet: {:#?}", packet);
                    if let Ok(data) = postcard::to_stdvec(&packet)
                        && writer.send(Bytes::from(data)).await.is_err() {
                            break;
                        }
                }
            }
        }
    }

    info!("Client disconnected!");
}
