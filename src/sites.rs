//! Supported-host registry and URL validation for the crawler.
//!
//! Both target sites (`metruyenhotne.com`, `metruyenhotvn.com`) share the same
//! backend template, so this module is the single point of truth that decides
//! whether a URL is accepted. Profiling for the design spec confirmed the rest
//! of the parser/metadata/discovery code is site-agnostic and needs no
//! per-host branching.

use anyhow::{Context, Result, anyhow};
use url::Url;

/// Hosts the crawler is willing to accept, listed alphabetically so error
/// messages render in a stable order across runs.
pub const SUPPORTED_HOSTS: &[&str] = &["metruyenhotne.com", "metruyenhotvn.com"];

/// Parse `url`, return its host lower-cased and with any leading `www.`
/// stripped. Returns an error if the URL cannot be parsed or has no host
/// component (e.g. a relative path or a bare scheme).
pub fn normalize_host(url: &str) -> Result<String> {
    let parsed = Url::parse(url).with_context(|| format!("invalid URL: {url}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL has no host: {url}"))?
        .to_ascii_lowercase();
    let trimmed = host.strip_prefix("www.").unwrap_or(&host);
    Ok(trimmed.to_string())
}

/// Normalise `url`'s host and require it to appear in [`SUPPORTED_HOSTS`].
/// Returns the normalised host on success. The error message names the
/// offending host and lists every supported host so users can correct the
/// input without consulting docs.
pub fn ensure_supported(url: &str) -> Result<String> {
    let host = normalize_host(url)?;
    if SUPPORTED_HOSTS.iter().any(|h| *h == host) {
        Ok(host)
    } else {
        Err(anyhow!(
            "Unsupported host '{host}'. Supported: {}.",
            SUPPORTED_HOSTS.join(", ")
        ))
    }
}

/// Like [`ensure_supported`] but also accepts `localhost` and `127.0.0.1`.
/// Used by the hidden `--allow-any-host` CLI flag for local fixtures and
/// integration tests that run against a `mockito` server.
pub fn ensure_supported_or_local(url: &str) -> Result<String> {
    let host = normalize_host(url)?;
    if host == "localhost" || host == "127.0.0.1" || SUPPORTED_HOSTS.iter().any(|h| *h == host) {
        Ok(host)
    } else {
        Err(anyhow!(
            "Unsupported host '{host}'. Supported: {} (or localhost / 127.0.0.1 with --allow-any-host).",
            SUPPORTED_HOSTS.join(", ")
        ))
    }
}

/// Validator-shaped wrapper: returns `None` when the URL is acceptable, or
/// `Some(error_message)` otherwise. Convenient for plugging into the wizard
/// text-prompt validator and for callers that want a string rather than an
/// `anyhow::Error`.
pub fn validate_url(url: &str, allow_any_host: bool) -> Option<String> {
    let check = if allow_any_host {
        ensure_supported_or_local
    } else {
        ensure_supported
    };
    check(url).err().map(|e| e.to_string())
}
