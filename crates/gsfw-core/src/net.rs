//! The HTTP adapter of the [`Source`] port: ureq with rustls and the operating system's root
//! certificates.

use alloc::sync::Arc;

use ureq::Agent;
use ureq::tls::{Certificate, RootCerts, TlsConfig, TlsProvider};

use crate::app::Source;

/// Downloads over HTTP and HTTPS.
pub struct HttpSource {
    agent: Agent,
}

impl HttpSource {
    /// An agent that trusts the root certificates of the operating system.
    ///
    /// When the operating system gives no certificates, HTTPS requests fail and plain HTTP
    /// requests still work.
    #[must_use]
    pub fn new() -> Self {
        let roots: Vec<Certificate<'static>> = rustls_native_certs::load_native_certs()
            .certs
            .iter()
            .map(|der| Certificate::from_der(der.as_ref()).to_owned())
            .collect();
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .root_certs(RootCerts::new_with_certs(&roots))
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        let agent = Agent::config_builder().tls_config(tls).build().new_agent();
        Self { agent }
    }
}

impl Default for HttpSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for HttpSource {
    fn get(&mut self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        let mut response = self.agent.get(url).call().map_err(|err| err.to_string())?;
        // ureq refuses a read after `limit` bytes, also the read that finds the end of the
        // body. With `limit + 1`, a body of exactly `limit` bytes passes.
        response
            .body_mut()
            .with_config()
            .limit(limit.saturating_add(1))
            .read_to_vec()
            .map_err(|err| err.to_string())
    }
}
