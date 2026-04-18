pub mod app;
pub mod config;
pub mod tui;
pub mod ui;

use std::{fs, path::PathBuf, sync::Arc};

use anyhow::Result;
use quinn::{ClientConfig, Endpoint};
use rustls::{
    DigitallySignedStruct,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
};

#[derive(Debug)]
pub struct TofuCertVerifier {
    pinned_cert_path: PathBuf,
    supported: WebPkiSupportedAlgorithms,
}

impl TofuCertVerifier {
    pub fn new(pinned_cert_path: PathBuf, supported: WebPkiSupportedAlgorithms) -> Self {
        Self {
            pinned_cert_path,
            supported,
        }
    }
}

impl ServerCertVerifier for TofuCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let cert_bytes = end_entity.as_ref();

        if self.pinned_cert_path.exists() {
            let pinned_bytes = fs::read(&self.pinned_cert_path).map_err(|e| {
                rustls::Error::General(format!("Failed to read pinned cert: {}", e))
            })?;
            if cert_bytes == pinned_bytes {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General(
                    "Certificate doesn't match. Attack?".into(),
                ))
            }
        } else {
            // if it already exists we trust and pin it
            if let Some(parent) = self.pinned_cert_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            fs::write(&self.pinned_cert_path, cert_bytes).map_err(|e| {
                rustls::Error::General(format!("Failed to write pinned cert: {}", e))
            })?;

            tracing::info!(
                "TOFU: Pinned new server certificate to {:?}",
                self.pinned_cert_path
            );

            Ok(ServerCertVerified::assertion())
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.supported.supported_schemes()
    }
}

pub fn get_endpoint() -> Result<Endpoint> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let home = std::env::var("HOME").expect("no HOME variable found in env");
    let pinned_cert_path = PathBuf::from(home)
        .join(".gader")
        .join("pinned_server.cert");

    let wpsa = rustls::crypto::ring::default_provider().signature_verification_algorithms;

    let verifier = Arc::new(TofuCertVerifier::new(pinned_cert_path, wpsa));

    let mut crypto = rustls::client::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    crypto.alpn_protocols = vec![b"gader-v1".to_vec()];

    let quic_client_config = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?;
    let client_config = ClientConfig::new(Arc::new(quic_client_config));

    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
}
