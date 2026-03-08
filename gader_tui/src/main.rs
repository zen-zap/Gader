#![allow(clippy::collapsible_if, clippy::collapsible_match)]
use std::net::SocketAddr;

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use gader_common::NetworkPacket;
use gader_tui::{
    app::{Action, App},
    config, get_endpoint, tui, ui,
};
use tokio_util::codec::{FramedRead, FramedWrite, length_delimited::LengthDelimitedCodec};
use tracing::{debug, error, info};

#[tokio::main]
async fn main() -> Result<()> {
    //tracing_subscriber::fmt::init(); // TODO: look into this -- make it write into a log file
    tui::install_panic_hook();
    let mut app = App::new();

    let client_secret = config::load_secret().context("Failed to load client secret")?;
    let endpoint = get_endpoint()?;

    let server_addr: SocketAddr = "127.0.0.1:23456".parse()?;

    let connection = endpoint
        .connect(server_addr, "localhost")?
        .await
        .context("Failed to connect to agent")?;

    info!("Connected to server at: {}", server_addr);

    let (send_stream, recv_stream) = connection
        .open_bi()
        .await
        .context("Failed to initiate bi-stream")?;

    debug!("Bi-directional stream successfully established");

    let mut terminal = tui::init_terminal()?;
    let mut key_reader = crossterm::event::EventStream::new();

    let mut writer = FramedWrite::new(send_stream, LengthDelimitedCodec::new());
    let mut reader = FramedRead::new(recv_stream, LengthDelimitedCodec::new());

    info!("Starting handshake");
    let handshake = NetworkPacket::Handshake {
        secret_token: client_secret,
    };

    writer
        .send(Bytes::from(postcard::to_stdvec(&handshake)?))
        .await?;

    match reader.next().await {
        Some(Ok(bytes)) => {
            if let Ok(NetworkPacket::HandshakeAck { accepted: true }) = postcard::from_bytes(&bytes)
            {
                info!("Handshake accepted! Starting TUI");
            } else {
                info!("CLIENT_SECRET rejected by server");
                bail!("Failed handshake");
            }
        }
        _ => {
            error!("Handshake with server failed! Connection Error");
            bail!("Handshake network error")
        }
    }

    info!("Listening for logs...");

    while !app.should_quit {
        terminal.draw(|f| ui::view(f, &mut app))?;

        tokio::select! {
            Some(msg_res) = reader.next() => {
                if let Ok(bytes) = msg_res {
                     if let Ok(packet) = postcard::from_bytes(&bytes) {
                         app.update(Action::Network(packet));
                     }
                }
            }

            Some(Ok(event)) = key_reader.next() => {
                match event {
                    crossterm::event::Event::Key(key) => {
                        if key.kind == crossterm::event::KeyEventKind::Press {
                            app.update(Action::Input(key.code));
                        }
                    }
                    crossterm::event::Event::Mouse(mouse) => {
                        match mouse.kind {
                            crossterm::event::MouseEventKind::ScrollUp => {
                                app.update(Action::Input(crossterm::event::KeyCode::Up));
                            }
                            crossterm::event::MouseEventKind::ScrollDown => {
                                app.update(Action::Input(crossterm::event::KeyCode::Down));
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        for packet in app.outbox.drain(..) {
            if let Ok(data) = postcard::to_stdvec(&packet) {
                writer.send(Bytes::from(data)).await.ok();
            }
        }
    }

    tui::restore_terminal()?;
    Ok(())
}
