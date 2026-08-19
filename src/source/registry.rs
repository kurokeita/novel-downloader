//! Host-to-adapter resolution. The single point that decides whether a URL
//! is accepted, replacing the old `sites::SUPPORTED_HOSTS` allowlist.

use anyhow::{Context, Result, anyhow};
use url::Url;

use super::SiteAdapter;
use super::khodocsach::Khodocsach;
use super::metruyenhot::Metruyenhot;

/// Every compiled-in adapter. Adding a site appends one entry here and
/// touches nothing else outside its own module.
static ADAPTERS: &[&dyn SiteAdapter] = &[&Khodocsach, &Metruyenhot];

/// Hosts accepted with `--allow-any-host`, for local fixtures and the
/// `mockito`-backed integration tests.
const LOCAL_HOSTS: &[&str] = &["localhost", "127.0.0.1"];

/// Parse `url`, return its host lower-cased and with any leading `www.`
/// stripped. Errors when the URL cannot be parsed or has no host component
/// (e.g. a relative path or a bare scheme).
pub fn normalize_host(url: &str) -> Result<String> {
    let parsed = Url::parse(url).with_context(|| format!("invalid URL: {url}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL has no host: {url}"))?
        .to_ascii_lowercase();
    let trimmed = host.strip_prefix("www.").unwrap_or(&host);
    Ok(trimmed.to_string())
}

/// Every host any adapter claims, sorted so error messages render in a
/// stable order across runs.
pub fn supported_hosts() -> Vec<&'static str> {
    let mut hosts: Vec<&'static str> = ADAPTERS
        .iter()
        .flat_map(|adapter| adapter.hosts().iter().copied())
        .collect();
    hosts.sort_unstable();
    hosts
}

/// Resolve `url`'s host to the adapter that owns it. `allow_any_host` also
/// accepts `localhost` / `127.0.0.1`, mapping them to the metruyenhot
/// adapter since local fixtures mirror its page layout.
pub fn resolve(url: &str, allow_any_host: bool) -> Result<&'static dyn SiteAdapter> {
    let host = normalize_host(url)?;
    if let Some(adapter) = ADAPTERS
        .iter()
        .find(|adapter| adapter.hosts().contains(&host.as_str()))
    {
        return Ok(*adapter);
    }
    if allow_any_host && LOCAL_HOSTS.contains(&host.as_str()) {
        return Ok(&Metruyenhot);
    }
    let supported = supported_hosts().join(", ");
    if allow_any_host {
        Err(anyhow!(
            "Unsupported host '{host}'. Supported: {supported} (or localhost / 127.0.0.1 with --allow-any-host)."
        ))
    } else {
        Err(anyhow!(
            "Unsupported host '{host}'. Supported: {supported}."
        ))
    }
}

/// Validator-shaped wrapper: `None` when the URL resolves, `Some(message)`
/// otherwise. Plugs into the wizard's text-prompt validator.
pub fn validate_url(url: &str, allow_any_host: bool) -> Option<String> {
    resolve(url, allow_any_host).err().map(|e| e.to_string())
}
