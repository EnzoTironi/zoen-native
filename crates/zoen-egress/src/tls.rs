//! TLS for the egress proxy (ADR 0028 §6, P1).
//!
//! - **Per-lease CA.** A sandbox whose tool has secrets bound to some hosts gets its own CA,
//!   created in this process when the lease registers. Only the certificate goes into the VM
//!   (added to its trust bundle); the key never leaves the proxy. The CA is name-constrained
//!   to the hosts the secrets are bound to and expires with the lease's week, so even a copy
//!   of it can't vouch for anything else.
//! - **Interception, only where needed.** A CONNECT to a host one of the lease's secrets is
//!   bound to is answered with a leaf certificate from that CA; the proxy reads the request,
//!   swaps `zoen-secret://<name>` placeholders for the values, and forwards it over a real,
//!   verified TLS connection. Every other CONNECT stays an opaque tunnel.
//! - **Browsers** can't take a new CA once they run, and a browser template's snapshot is
//!   taken before any lease exists. So the node has one [`SandboxRoot`] per egress process,
//!   baked into browser templates; a browser lease's CA is an intermediate issued under it,
//!   still name-constrained to that lease's hosts. Only this proxy holds the root's key, and
//!   a VM's traffic reaches no other proxy, so trusting the root lets nobody else in.
//! - **Upstream verification** uses the Mozilla roots (`webpki-roots`), plus extra roots only
//!   when a test configures them.

use crate::policy::host_matches;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
    GeneralSubtree, IsCa, Issuer, KeyPair, KeyUsagePurpose, NameConstraints, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The node's root for browser leases (see the module docs). Lives as long as the egress
/// process; browser templates are rebuilt with it on every start.
pub struct SandboxRoot {
    cert_pem: String,
    issuer: Issuer<'static, KeyPair>,
}

impl std::fmt::Debug for SandboxRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SandboxRoot").finish_non_exhaustive()
    }
}

impl SandboxRoot {
    pub fn new() -> Result<Self, String> {
        let key = KeyPair::generate().map_err(|e| e.to_string())?;
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "Zoen sandbox root");
        dn.push(DnType::OrganizationName, "Zoen egress");
        params.distinguished_name = dn;
        // Lease CAs only, and they can't issue further CAs.
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(1));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::hours(1);
        params.not_after = now + time::Duration::days(30);
        let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
        Ok(SandboxRoot {
            cert_pem: cert.pem(),
            issuer: Issuer::new(params, key),
        })
    }

    /// The certificate browser templates trust. Never the key.
    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }
}

/// One lease's CA. Leaf certificates are made on first use per host and cached.
pub struct LeaseCa {
    cert_pem: String,
    cert_der: CertificateDer<'static>,
    issuer: Issuer<'static, KeyPair>,
    hosts: Vec<String>,
    leaves: Mutex<HashMap<String, Arc<rustls::ServerConfig>>>,
}

impl std::fmt::Debug for LeaseCa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeaseCa").finish_non_exhaustive()
    }
}

/// `*.example.com` → `example.com` (a DNS subtree covers the name and everything below it).
fn subtree(host: &str) -> String {
    host.trim_start_matches("*.").to_ascii_lowercase()
}

impl LeaseCa {
    /// A CA for `lease` that can only vouch for `hosts` (and their subdomains): self-signed,
    /// or issued under `root` for browser leases.
    pub fn new(lease: &str, hosts: &[String], root: Option<&SandboxRoot>) -> Result<Self, String> {
        let key = KeyPair::generate().map_err(|e| e.to_string())?;
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, format!("Zoen sandbox CA {lease}"));
        dn.push(DnType::OrganizationName, "Zoen egress");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params.name_constraints = Some(NameConstraints {
            permitted_subtrees: hosts
                .iter()
                .map(|h| GeneralSubtree::DnsName(subtree(h)))
                .collect(),
            excluded_subtrees: vec![],
        });
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::hours(1);
        params.not_after = now + time::Duration::days(7);
        let cert = match root {
            None => params.self_signed(&key),
            Some(r) => {
                params.use_authority_key_identifier_extension = true;
                params.signed_by(&key, &r.issuer)
            }
        }
        .map_err(|e| e.to_string())?;
        Ok(LeaseCa {
            cert_pem: cert.pem(),
            cert_der: cert.der().clone(),
            issuer: Issuer::new(params, key),
            hosts: hosts.to_vec(),
            leaves: Mutex::new(HashMap::new()),
        })
    }

    /// Whether this CA's name constraints cover `host` (only those hosts are intercepted).
    pub fn covers(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.hosts.iter().any(|h| {
            let sub = subtree(h);
            host == sub || host_matches(&format!("*.{sub}"), &host)
        })
    }

    /// The certificate the sandbox trusts. Never the key.
    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    /// A leaf for `host` and the chain to present (leaf, then this CA).
    fn issue(
        &self,
        host: &str,
    ) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), String> {
        let key = KeyPair::generate().map_err(|e| e.to_string())?;
        let mut params =
            CertificateParams::new(vec![host.to_string()]).map_err(|e| e.to_string())?;
        params.subject_alt_names = vec![SanType::DnsName(
            host.to_string()
                .try_into()
                .map_err(|_| "bad host name".to_string())?,
        )];
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, host);
        params.distinguished_name = dn;
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::hours(1);
        params.not_after = now + time::Duration::days(7);
        params.use_authority_key_identifier_extension = true;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let leaf = params
            .signed_by(&key, &self.issuer)
            .map_err(|e| e.to_string())?;
        Ok((
            vec![leaf.der().clone(), self.cert_der.clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
        ))
    }

    /// The TLS server side for `host`, presented to the sandbox.
    pub fn acceptor(&self, host: &str) -> Result<TlsAcceptor, String> {
        let host = host.to_ascii_lowercase();
        if let Some(c) = self.leaves.lock().unwrap().get(&host) {
            return Ok(TlsAcceptor::from(c.clone()));
        }
        let (chain, key) = self.issue(&host)?;
        let mut cfg = rustls::ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .map_err(|e| e.to_string())?;
        // We read and rewrite HTTP/1.1 only.
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
        let cfg = Arc::new(cfg);
        self.leaves.lock().unwrap().insert(host, cfg.clone());
        Ok(TlsAcceptor::from(cfg))
    }
}

/// The client side towards real origins.
#[derive(Clone)]
pub struct Upstream {
    connector: TlsConnector,
}

impl std::fmt::Debug for Upstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Upstream").finish_non_exhaustive()
    }
}

impl Upstream {
    /// Mozilla's roots plus `extra_roots_pem` (tests only).
    pub fn new(extra_roots_pem: &[String]) -> Result<Self, String> {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        for pem in extra_roots_pem {
            for der in pem_certs(pem)? {
                roots.add(der).map_err(|e| e.to_string())?;
            }
        }
        let mut cfg = rustls::ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Upstream {
            connector: TlsConnector::from(Arc::new(cfg)),
        })
    }

    pub async fn connect(
        &self,
        host: &str,
        tcp: TcpStream,
    ) -> std::io::Result<tokio_rustls::client::TlsStream<TcpStream>> {
        let name = ServerName::try_from(host.to_string())
            .map_err(|_| std::io::Error::other("bad server name"))?;
        self.connector.connect(name, tcp).await
    }
}

/// DER certificates in a PEM string.
pub fn pem_certs(pem: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    use rustls::pki_types::pem::PemObject;
    CertificateDer::pem_slice_iter(pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lease_ca_signs_only_for_its_hosts() {
        let ca = LeaseCa::new("lease_x", &["api.example.test".into()], None).unwrap();
        assert!(ca.covers("api.example.test") && !ca.covers("other.test"));
        assert!(ca.cert_pem().starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(!ca.cert_pem().contains("PRIVATE KEY"));
        assert!(ca.acceptor("api.example.test").is_ok());
        assert_eq!(pem_certs(ca.cert_pem()).unwrap().len(), 1);
    }

    #[test]
    fn a_browser_lease_ca_chains_to_the_root_and_stays_constrained() {
        use rustls::client::danger::ServerCertVerifier;
        let root = SandboxRoot::new().unwrap();
        let ca = LeaseCa::new("lease_b", &["*.shop.test".into()], Some(&root)).unwrap();
        assert!(ca.covers("www.shop.test") && ca.covers("shop.test") && !ca.covers("bank.test"));
        // A client that trusts only the root accepts the lease's leaf for a covered host...
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(pem_certs(root.cert_pem()).unwrap().remove(0))
            .unwrap();
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            provider(),
        )
        .build()
        .unwrap();
        let now = rustls::pki_types::UnixTime::now();
        let check = |host: &str| {
            let (chain, _) = ca.issue(host).unwrap();
            verifier.verify_server_cert(
                &chain[0],
                &chain[1..],
                &ServerName::try_from(host.to_string()).unwrap(),
                &[],
                now,
            )
        };
        assert!(check("www.shop.test").is_ok());
        // ...and refuses one outside its name constraints, even though the CA signed it.
        assert!(check("bank.test").is_err());
    }
}
