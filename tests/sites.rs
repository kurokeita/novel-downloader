use truyenazz_crawler::sites::{
    SUPPORTED_HOSTS, ensure_supported, ensure_supported_or_local, normalize_host, validate_url,
};

#[test]
fn supported_hosts_listed_alphabetically_for_stable_error_messages() {
    assert_eq!(SUPPORTED_HOSTS, &["metruyenhotne.com", "metruyenhotvn.com"]);
}

#[test]
fn normalize_host_lowercases_and_strips_www() {
    assert_eq!(
        normalize_host("https://WWW.MetruyenhotVN.com/foo").unwrap(),
        "metruyenhotvn.com"
    );
}

#[test]
fn normalize_host_keeps_apex_host_unchanged() {
    assert_eq!(
        normalize_host("https://metruyenhotvn.com/bar/").unwrap(),
        "metruyenhotvn.com"
    );
}

#[test]
fn normalize_host_errors_on_url_without_host() {
    let err = normalize_host("not-a-url").unwrap_err();
    assert!(err.to_string().to_lowercase().contains("url"), "got: {err}");
}

#[test]
fn ensure_supported_accepts_each_listed_host() {
    for host in SUPPORTED_HOSTS {
        let url = format!("https://{host}/some-novel");
        let resolved = ensure_supported(&url)
            .unwrap_or_else(|e| panic!("expected {host} to be accepted, got {e}"));
        assert_eq!(&resolved, host);
    }
}

#[test]
fn ensure_supported_accepts_www_prefixed_host() {
    let host = ensure_supported("https://www.metruyenhotne.com/foo").unwrap();
    assert_eq!(host, "metruyenhotne.com");
}

#[test]
fn ensure_supported_rejects_unknown_host_and_lists_supported() {
    let err = ensure_supported("https://example.com/foo")
        .unwrap_err()
        .to_string();
    assert!(err.contains("example.com"), "got: {err}");
    for host in SUPPORTED_HOSTS {
        assert!(err.contains(host), "missing {host} in error: {err}");
    }
}

#[test]
fn ensure_supported_or_local_accepts_localhost() {
    assert_eq!(
        ensure_supported_or_local("http://localhost:8765/foo").unwrap(),
        "localhost"
    );
    assert_eq!(
        ensure_supported_or_local("http://127.0.0.1:8765/foo").unwrap(),
        "127.0.0.1"
    );
}

#[test]
fn ensure_supported_or_local_still_rejects_unrelated_host() {
    assert!(ensure_supported_or_local("https://example.com/foo").is_err());
}

#[test]
fn validate_url_returns_none_on_supported_host() {
    assert!(validate_url("https://metruyenhotvn.com/foo", false).is_none());
}

#[test]
fn validate_url_returns_error_message_on_unknown_host() {
    let msg = validate_url("https://example.com/foo", false).unwrap();
    assert!(msg.contains("example.com"));
    assert!(msg.contains("metruyenhotvn.com"));
}

#[test]
fn validate_url_with_allow_any_host_accepts_localhost() {
    assert!(validate_url("http://localhost:8765/foo", true).is_none());
}

#[test]
fn validate_url_with_allow_any_host_still_rejects_unsupported() {
    assert!(validate_url("https://example.com/foo", true).is_some());
}
