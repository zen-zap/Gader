#![allow(clippy::collapsible_if, clippy::collapsible_match)]
use std::net::SocketAddr;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use gader_common::NetworkPacket;
use gader_tui::{
    app::{Action, App},
    get_endpoint, tui, ui,
};
use tokio_util::codec::{FramedRead, FramedWrite, length_delimited::LengthDelimitedCodec};
use tracing::{debug, info};

#[tokio::main]
async fn main() -> Result<()> {
    //tracing_subscriber::fmt::init(); // TODO: look into this
    tui::install_panic_hook();
    let mut terminal = tui::init_terminal()?;
    let mut key_reader = crossterm::event::EventStream::new();
    let mut app = App::new();

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

    let mut writer = FramedWrite::new(send_stream, LengthDelimitedCodec::new());
    let mut reader = FramedRead::new(recv_stream, LengthDelimitedCodec::new());

    // TODO: ideally this should be a handshake -- saved implement later
    let init_packet = NetworkPacket::KeepAlive;
    let init_bytes = postcard::to_stdvec(&init_packet)?;
    writer.send(Bytes::from(init_bytes)).await?;

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
    }

    tui::restore_terminal()?;
    Ok(())
}
