use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tracing::info;

fn cert_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME environment variable not set")?;
    let dir = PathBuf::from(home).join(".gader");
    fs::create_dir_all(&dir).context("Failed to create ~/.gader directory")?;
    Ok(dir)
}

/// If a certificate exists, we use that else we generate a new one
/// certificate pinning is done on the client side
pub fn load_or_generate_keys() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let dir = cert_dir()?;
    let cert_path = dir.join("server.cert");
    let key_path = dir.join("server.key");

    if cert_path.exists() && key_path.exists() {
        info!("Loading existing TLS keys from {}", dir.display());

        let cert_der = fs::read(&cert_path).context("Unable to read cert")?;
        let key_der = fs::read(&key_path).context("Unable to read key")?;

        return Ok((
            vec![CertificateDer::from(cert_der)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
        ));
    }

    info!("Generating new self-signed TLS keys in {}", dir.display());

    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(vec!["gader_server".into()])
            .context("Unable to generate self-signed certificate")?;

    let cert_der = cert.der().clone();
    let key_der_vec = signing_key.serialize_der();

    fs::write(&cert_path, cert_der.as_ref()).context("Failed to save cert")?;
    fs::write(&key_path, &key_der_vec).context("Failed to save key")?;

    Ok((
        vec![cert_der],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der_vec)),
    ))
}
