use std::{fs, path::Path};

use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

pub fn load_or_generate_keys() -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
    // update paths later
    let cert_path = Path::new("server.cert");
    let key_path = Path::new("server.key");

    if cert_path.exists() && key_path.exists() {
        println!("Loading existing keys"); // remove this later

        let cert_der = fs::read(cert_path).expect("unable to read cert");
        let key_der = fs::read(key_path).expect("unable to read key");

        return (
            vec![CertificateDer::from(cert_der)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
        );
    }

    println!("Generating new self-signed keys");

    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(vec!["gader_server".into()])
            .expect("unable to generate certificate");

    let cert_der = cert.der().clone();
    let key_der_vec = signing_key.serialize_der();

    fs::write(cert_path, cert_der.as_ref()).expect("failed to save cert");
    fs::write(key_path, &key_der_vec).expect("failed to save key");

    (
        vec![cert_der],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der_vec)),
    )
}
