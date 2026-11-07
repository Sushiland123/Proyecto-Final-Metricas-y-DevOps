//! Fail-closed API-key RBAC. Compare fixed-size digests in constant time.
use crate::{config::Settings, error::Error};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use subtle::ConstantTimeEq;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Producer,
    Analyst,
}
#[derive(Clone)]
pub struct Principal {
    role: Role,
    sources: Option<BTreeSet<String>>,
}
impl Principal {
    pub fn require(&self, role: Role) -> Result<(), Error> {
        if self.role == role {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
    pub fn require_source(&self, source: &str) -> Result<(), Error> {
        self.require(Role::Producer)?;
        if self
            .sources
            .as_ref()
            .is_none_or(|sources| sources.contains(source))
        {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
}
fn equal(a: &str, b: &str) -> bool {
    bool::from(Sha256::digest(a.as_bytes()).ct_eq(&Sha256::digest(b.as_bytes())))
}
pub fn authenticate(key: Option<&str>, settings: &Settings) -> Result<Principal, Error> {
    let reject = Error::Unauthenticated;
    let producer = settings
        .producer_key
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or(reject)?;
    let analyst = settings
        .analyst_key
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or(reject)?;
    if settings.auth_invalid
        || equal(producer, analyst)
        || settings
            .source_keys
            .values()
            .any(|s| s.trim().is_empty() || equal(s, producer) || equal(s, analyst))
    {
        return Err(reject);
    }
    let key = key.filter(|s| !s.trim().is_empty()).ok_or(reject)?;
    let producer_match = equal(key, producer);
    let analyst_match = equal(key, analyst);
    let sources: BTreeSet<_> = settings
        .source_keys
        .iter()
        .filter(|(_, secret)| equal(key, secret))
        .map(|(source, _)| source.clone())
        .collect();
    if producer_match {
        Ok(Principal {
            role: Role::Producer,
            sources: None,
        })
    } else if analyst_match {
        Ok(Principal {
            role: Role::Analyst,
            sources: None,
        })
    } else if !sources.is_empty() {
        Ok(Principal {
            role: Role::Producer,
            sources: Some(sources),
        })
    } else {
        Err(reject)
    }
}
