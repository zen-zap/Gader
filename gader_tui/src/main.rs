use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use gader_common::NetworkPacket;
use quinn::{ClientConfig, Endpoint};
use rustls::{
    DigitallySignedStruct,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use tokio_util::codec::{FramedRead, FramedWrite, length_delimited::LengthDelimitedCodec};

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // this gives a ConfigBuilder
    let mut crypto = rustls::client::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    crypto.alpn_protocols = vec![b"gader-v1".to_vec()];

    // this implements quinn::crypto::ClientConfig
    let quic_client_config = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?;

    let client_config = ClientConfig::new(Arc::new(quic_client_config));

    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_config);

    let server_addr: SocketAddr = "127.0.0.1:23456".parse()?;
    println!("Connecting to {}...", server_addr);

    let connection = endpoint
        .connect(server_addr, "localhost")?
        .await
        .context("Failed to connect to agent")?;

    println!("Connected! Initiating bidirectional stream...");

    let (send_stream, recv_stream) = connection
        .open_bi()
        .await
        .context("Failed to initiate bi-stream")?;

    let mut writer = FramedWrite::new(send_stream, LengthDelimitedCodec::new());
    let mut reader = FramedRead::new(recv_stream, LengthDelimitedCodec::new());

    //println!("Sending Filter Command: immich_server");
    // let filter_cmd = NetworkPacket::UpdateFilter {
    //     service: Some("immich_server".to_string()),
    //     level: None,
    // };
    //
    // let cmd_bytes = postcard::to_stdvec(&filter_cmd)?;
    // writer.send(Bytes::from(cmd_bytes)).await?;

    println!("Listening for logs...");
    while let Some(msg) = reader.next().await {
        match msg {
            Ok(bytes) => {
                if let Ok(packet) = postcard::from_bytes::<NetworkPacket>(&bytes) {
                    match packet {
                        NetworkPacket::Batch(logs) => {
                            println!("getting a network packet");
                            for log in logs {
                                println!("[{}] {}", log.service, log.message);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => eprintln!("Error reading frame: {}", e),
        }
    }

    Ok(())
}

#[derive(Debug)]
struct SkipServerVerification;

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA1,
            rustls::SignatureScheme::ECDSA_SHA1_Legacy,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}
