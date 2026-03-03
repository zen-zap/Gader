use core::{
    convert::TryInto,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};
use std::sync::Arc;

use anyhow::{Context, Result};
use bollard::{API_DEFAULT_VERSION, Docker, query_parameters::LogsOptionsBuilder};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use gader_agent::{
    cert,
    parsers::{LogParser, immich, vaultwarden},
};
use gader_common::{LogEntry, NetworkPacket};
use quinn::{Endpoint, ServerConfig, TransportConfig, VarInt};
use tokio::sync::broadcast;
use tokio_util::codec::{FramedRead, FramedWrite, length_delimited::LengthDelimitedCodec};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let docker_conn = Docker::connect_with_http("http://127.0.0.1:2375", 5, API_DEFAULT_VERSION)
        .expect("Unable to connect to docker");

    // services to watch out for as of now
    // immich and vaultwarden

    // server endpoint for accepting connections
    let server_endpoint = get_connection_endpoint().context("Error in making endpoint")?;

    println!("got here");

    let (tx, _) = broadcast::channel::<LogEntry>(1000);

    // clones are cheap here
    let tx_immich = tx.clone();
    let docker_immich = docker_conn.clone();

    let _task_immich = tokio::spawn(async move {
        let immich_parser = immich::ImmichParser::new();

        spawn_watcher(docker_immich, "immich_server", immich_parser, tx_immich).await;
    });

    let tx_vw = tx.clone();
    let docker_vw = docker_conn.clone();

    let _task_vw = tokio::spawn(async move {
        let vw_parser = vaultwarden::VWParser::new();
        spawn_watcher(docker_vw, "vaultwarden", vw_parser, tx_vw).await;
    });

    let tx_ntwk = tx.clone();
    tokio::spawn(async move {
        println!("Awaiting connections");
        while let Some(conn) = server_endpoint.accept().await {
            println!("Accepting a client");
            let tx_curr_client = tx_ntwk.clone();

            tokio::spawn(async move {
                handle_client(conn, tx_curr_client).await;
            });
        }
    });

    tokio::signal::ctrl_c().await?;
    println!("Shutting down ...");

    Ok(())
}

fn get_connection_endpoint() -> Result<Endpoint> {
    let (cert_chain, key_der) = cert::load_or_generate_keys();

    // ServerConfig: common configuration for a set of server sessions

    // we create a server configuration with a crypto provider
    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)
        .context("failed to build TLS config")?;

    // setup ALPN - application layer protocol negotiation
    crypto.alpn_protocols = vec![b"gader-v1".to_vec()];

    // this returns an Arc<ServerConfig> .. can we use it directly?
    let quic_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(crypto)?;

    // this would be local host?
    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 23456);

    let mut transport_config = TransportConfig::default();
    transport_config.max_concurrent_bidi_streams(VarInt::from_u32(10).into());
    transport_config.max_idle_timeout(Some(Duration::from_secs(30).try_into()?));
    transport_config.keep_alive_interval(Some(Duration::from_secs(20).try_into()?));

    let mut quic_server_config = ServerConfig::with_crypto(Arc::new(quic_crypto));

    quic_server_config.transport_config(Arc::new(transport_config));

    // endpoint requires a Arc<dyn ServerConfig>, SocketAddr
    let server_endpoint = Endpoint::server(quic_server_config, socket_addr)?;

    Ok(server_endpoint)
}

async fn spawn_watcher<P: LogParser>(
    docker: Docker,
    name: &str,
    parser: P,
    tx: broadcast::Sender<LogEntry>,
) {
    println!("Watching: {}", name);
    let params = LogsOptionsBuilder::new()
        .follow(true)
        .stderr(true)
        .stdout(true)
        .tail("30")
        .build();

    let mut stream = docker.logs(name, Some(params));

    while let Some(Ok(log)) = stream.next().await {
        println!("{:?}", log);

        if let Some(entry) = parser.parse(&log.to_string()) {
            println!("{:?}", entry);
            let _ = tx.send(entry);
        }
    }
}

async fn handle_client(conn: quinn::Incoming, tx: broadcast::Sender<LogEntry>) {
    println!("got here");

    let connection = match conn.await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Handshake failed: {}", e);
            return;
        }
    };

    println!("Client connected: {}", connection.remote_address());

    let (send_stream, recv_stream) = match connection.accept_bi().await {
        Ok(s) => {
            println!("Received bi-stream: {:#?}", s);
            s
        }
        Err(e) => {
            eprintln!("Failed to accept bi-stream: {}", e);
            return;
        }
    };

    let mut writer = FramedWrite::new(send_stream, LengthDelimitedCodec::new());
    let mut reader = FramedRead::new(recv_stream, LengthDelimitedCodec::new());

    let mut rx = tx.subscribe();
    // we have to subscribe to the broadcast channel
    let mut batch: Vec<LogEntry> = Vec::with_capacity(10);

    // upgrade this an enum later on
    // to filter on different log levels and services or timestamps etc.
    // default is everything
    let mut filter: Option<String> = None;

    let mut flush_timer = tokio::time::interval(tokio::time::Duration::from_millis(500));

    println!("Got here!");

    loop {
        tokio::select! {

            // TODO: add proper filter support later on

            // sending stuff
            msg = rx.recv() => {

                // run it if we receive something in the broadcast channel
                println!("inside rx.recv()");

                match msg {
                    Ok(entry) => {
                        if let Some(ref svc) = filter
                            && !entry.service.eq_ignore_ascii_case(svc) {
                                continue;
                            }

                        batch.push(entry);

                        // make the max batch length configurable later on
                        if batch.len() >= 10 {
                            let batch_to_send = std::mem::take(&mut batch);
                            let packet = NetworkPacket::Batch(batch_to_send);

                            println!("Sending packet: {:#?}", packet);

                            if let Ok(data) = postcard::to_stdvec(&packet)
                                && writer.send(Bytes::from(data)).await.is_err() {
                                    break;
                                }
                        }
                    }
                    Err(e) => {
                        println!("Encountered Error: {:?}", e);
                    }
                }
            }

            // recv stuff
            packet_res = reader.next() => {

                match packet_res {
                    Some(Ok(bytes)) => {
                        if let Ok(packet) = postcard::from_bytes::<NetworkPacket>(&bytes)
                            && let NetworkPacket::UpdateFilter {
                                    service,
                                    ..
                                } = packet {
                                println!("Updating filter to: {:?}", service);
                                filter = service;
                            }
                    }
                    Some(Err(e)) => {
                        eprintln!("Framing Error: {}", e);
                        break;
                    }
                    None => break,
                }
            }

            _ = flush_timer.tick() => {

                if !batch.is_empty() {

                    let batch_to_send = std::mem::take(&mut batch);
                    let packet = NetworkPacket::Batch(batch_to_send);
                    println!("Sending packet: {:#?}", packet);
                    if let Ok(data) = postcard::to_stdvec(&packet)
                        && writer.send(Bytes::from(data)).await.is_err() {
                            break;
                        }
                }
            }
        }
    }

    println!("Client disconnected!");
}
