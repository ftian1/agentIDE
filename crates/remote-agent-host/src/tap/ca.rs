//! Certificate authority for the HTTP MITM tap.
//!
//! Generates (once) a self-signed CA persisted to `~/.remote-agent-host/tap-ca/`,
//! then mints per-host leaf certs signed by that CA on demand, cached in memory.
//! The CA cert (PEM) is injected into the agent CLI via `NODE_EXTRA_CA_CERTS` so
//! the Node-based CLIs trust our interception leaf certs.

use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose,
    SanType,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// A minted leaf: the rustls-ready cert chain + signing key for one host.
#[derive(Clone)]
pub struct LeafCert {
    pub chain: Vec<CertificateDer<'static>>,
    /// PKCS#8 DER of the leaf private key (rebuilt into `PrivateKeyDer` per use,
    /// since `PrivateKeyDer` itself is not `Clone`).
    pub key_der: Vec<u8>,
}

impl LeafCert {
    /// Build a fresh `PrivateKeyDer` from the stored DER bytes.
    pub fn key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::try_from(self.key_der.clone())
            .expect("leaf key DER was valid at mint time")
    }
}

/// The tap certificate authority. Cheap to clone (`Arc` inside).
#[derive(Clone)]
pub struct TapCa {
    inner: Arc<CaInner>,
}

struct CaInner {
    ca_cert: Certificate,
    ca_key: KeyPair,
    ca_pem_path: PathBuf,
    leaves: DashMap<String, LeafCert>,
}

impl TapCa {
    /// Load the persisted CA, or generate + persist a new one on first run.
    pub fn load_or_create() -> anyhow::Result<Self> {
        let dir = ca_dir();
        std::fs::create_dir_all(&dir)?;
        let cert_path = dir.join("ca.pem");
        let key_path = dir.join("ca.key");

        let (ca_cert, ca_key) = if cert_path.exists() && key_path.exists() {
            let cert_pem = std::fs::read_to_string(&cert_path)?;
            let key_pem = std::fs::read_to_string(&key_path)?;
            let ca_key = KeyPair::from_pem(&key_pem)
                .map_err(|e| anyhow::anyhow!("parse CA key: {e}"))?;
            let params = CertificateParams::from_ca_cert_pem(&cert_pem)
                .map_err(|e| anyhow::anyhow!("parse CA cert: {e}"))?;
            let ca_cert = params
                .self_signed(&ca_key)
                .map_err(|e| anyhow::anyhow!("rebuild CA cert: {e}"))?;
            (ca_cert, ca_key)
        } else {
            let (ca_cert, ca_key) = generate_ca()?;
            std::fs::write(&cert_path, ca_cert.pem())?;
            std::fs::write(&key_path, ca_key.serialize_pem())?;
            (ca_cert, ca_key)
        };

        Ok(Self {
            inner: Arc::new(CaInner {
                ca_cert,
                ca_key,
                ca_pem_path: cert_path,
                leaves: DashMap::new(),
            }),
        })
    }

    /// Path to the CA cert PEM (for `NODE_EXTRA_CA_CERTS`).
    pub fn ca_pem_path(&self) -> &std::path::Path {
        &self.inner.ca_pem_path
    }

    /// Get (or mint + cache) a leaf certificate for the given host.
    pub fn leaf_for(&self, host: &str) -> anyhow::Result<LeafCert> {
        if let Some(leaf) = self.inner.leaves.get(host) {
            return Ok(leaf.clone());
        }
        let leaf = self.mint_leaf(host)?;
        self.inner.leaves.insert(host.to_string(), leaf.clone());
        Ok(leaf)
    }

    fn mint_leaf(&self, host: &str) -> anyhow::Result<LeafCert> {
        let mut params = CertificateParams::new(vec![host.to_string()])
            .map_err(|e| anyhow::anyhow!("leaf params: {e}"))?;
        params
            .distinguished_name
            .push(DnType::CommonName, host);
        params.subject_alt_names = vec![san_for(host)];
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];

        let leaf_key = KeyPair::generate().map_err(|e| anyhow::anyhow!("leaf key: {e}"))?;
        let leaf_cert = params
            .signed_by(&leaf_key, &self.inner.ca_cert, &self.inner.ca_key)
            .map_err(|e| anyhow::anyhow!("sign leaf: {e}"))?;

        let chain = vec![
            leaf_cert.der().clone(),
            self.inner.ca_cert.der().clone(),
        ];
        let key_der = leaf_key.serialize_der();
        // Validate once at mint time so `LeafCert::key()` can unwrap safely.
        PrivateKeyDer::try_from(key_der.clone())
            .map_err(|e| anyhow::anyhow!("leaf key der: {e}"))?;

        Ok(LeafCert { chain, key_der })
    }
}

/// Build a SAN entry: IP-literal hosts become IpAddress SANs, others DnsName.
fn san_for(host: &str) -> SanType {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        SanType::IpAddress(ip)
    } else {
        // DnsName::try_from only fails on invalid chars; fall back to a literal.
        match host.to_string().try_into() {
            Ok(dns) => SanType::DnsName(dns),
            Err(_) => SanType::DnsName("invalid.local".to_string().try_into().unwrap()),
        }
    }
}

fn generate_ca() -> anyhow::Result<(Certificate, KeyPair)> {
    let mut params =
        CertificateParams::new(Vec::<String>::new()).map_err(|e| anyhow::anyhow!("ca params: {e}"))?;
    params
        .distinguished_name
        .push(DnType::CommonName, "Remote AI IDE Tap CA");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Remote AI IDE");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];

    let key = KeyPair::generate().map_err(|e| anyhow::anyhow!("ca key: {e}"))?;
    let cert = params
        .self_signed(&key)
        .map_err(|e| anyhow::anyhow!("self-sign CA: {e}"))?;
    Ok((cert, key))
}

fn ca_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".remote-agent-host").join("tap-ca")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_ca_and_mints_cached_leaf() {
        super::super::proxy::ensure_crypto_provider_for_test();
        let (ca_cert, ca_key) = generate_ca().expect("generate CA");
        let ca = TapCa {
            inner: Arc::new(CaInner {
                ca_cert,
                ca_key,
                ca_pem_path: PathBuf::from("/tmp/unused-ca.pem"),
                leaves: DashMap::new(),
            }),
        };

        let leaf1 = ca.leaf_for("api.anthropic.com").expect("mint leaf");
        // Chain is [leaf, ca]; key DER round-trips into a PrivateKeyDer.
        assert_eq!(leaf1.chain.len(), 2);
        let _ = leaf1.key();

        // Second call for the same host hits the cache (same key bytes).
        let leaf2 = ca.leaf_for("api.anthropic.com").expect("cached leaf");
        assert_eq!(leaf1.key_der, leaf2.key_der);

        // An IP-literal host mints a distinct leaf without error.
        let leaf_ip = ca.leaf_for("127.0.0.1").expect("ip leaf");
        assert_eq!(leaf_ip.chain.len(), 2);
    }
}
