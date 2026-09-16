mod download;

pub use download::download_mod_file;
#[cfg(test)]
pub(crate) use download::{extract_extension_from_content_disposition, sniff_extension_from_file};

use crate::error::{AppError, AppResult};
use crate::state::SettingsState;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use tauri::{Emitter, Manager};
use ts_rs::TS;
use url::Url;

/// A `ltk://` deep link, as the route named in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeepLinkRequest {
    Install(DeepLinkInstallRequest),
    Settings(DeepLinkSettingsRequest),
}

impl DeepLinkRequest {
    /// Sends the route to the frontend, on the event that route is heard on.
    fn emit_to(&self, app_handle: &tauri::AppHandle) {
        let _ = match self {
            Self::Install(request) => app_handle.emit("deep-link-install", request),
            Self::Settings(request) => app_handle.emit("deep-link-settings", request),
        };
    }
}

/// Parsed representation of a `ltk://install` deep-link URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkInstallRequest {
    pub url: String,
    pub name: Option<String>,
    pub author: Option<String>,
    pub source: Option<String>,
    /// The host outside the allowlist, or `None` where the allowlist covers it.
    ///
    /// Stamped by [`handle_single`] rather than by parsing, since the trust is a
    /// property of the reader's settings and not of the URL.
    pub untrusted_domain: Option<String>,
}

/// Parsed representation of a `ltk://settings` deep-link URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkSettingsRequest {
    /// The public setting or group id the page opens on, as `?focus=` carries it.
    pub focus: String,
}

/// Progress payload emitted during protocol install download.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolInstallProgress {
    pub stage: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
}

/// A link that arrived before the frontend could hear it, and whether one still could.
#[derive(Debug, Default)]
struct Handoff {
    listening: bool,
    pending: Option<DeepLinkRequest>,
}

/// Rate-limiter and hand-off state for deep-link invocations.
pub struct DeepLinkState {
    last_invocation: Mutex<Option<Instant>>,
    handoff: Mutex<Handoff>,
}

impl DeepLinkState {
    pub fn new() -> Self {
        Self {
            last_invocation: Mutex::new(None),
            handoff: Mutex::new(Handoff::default()),
        }
    }

    /// Returns `true` if the invocation should be dropped (rate-limited).
    pub fn should_rate_limit(&self) -> bool {
        let mut last = self.last_invocation.lock();
        let now = Instant::now();
        if let Some(prev) = *last {
            if now.duration_since(prev).as_secs_f64() < 1.0 {
                return true;
            }
        }
        *last = Some(now);
        false
    }

    /// Sends the link on, or holds it until the frontend asks for one.
    ///
    /// A URL handed to a cold start arrives before the window's script has run, so
    /// the event carrying it would reach nobody. Held and sent under the one lock
    /// [`Self::take_pending`] drains, so a link cannot fall between the two.
    pub fn deliver(&self, app_handle: &tauri::AppHandle, request: DeepLinkRequest) {
        let mut handoff = self.handoff.lock();
        if !handoff.listening {
            handoff.pending = Some(request);
            return;
        }

        crate::commands::shell::raise_main_window(app_handle);
        request.emit_to(app_handle);
    }

    /// The held link, if there is one, and marks the frontend listening from here on.
    pub fn take_pending(&self) -> Option<DeepLinkRequest> {
        let mut handoff = self.handoff.lock();
        handoff.listening = true;
        handoff.pending.take()
    }
}

/// The link that arrived before the frontend was listening, if there was one.
///
/// The window is created hidden, so a link the frontend is only now hearing about
/// has nothing on screen under it yet.
pub fn take_pending(app_handle: &tauri::AppHandle) -> Option<DeepLinkRequest> {
    let state: tauri::State<'_, DeepLinkState> = app_handle.state();
    let pending = state.take_pending();
    if pending.is_some() {
        crate::commands::shell::raise_main_window(app_handle);
    }
    pending
}

/// Parse and validate a `ltk://` deep-link URL into the route it names.
///
/// # Errors
///
/// Returns [`AppError::ValidationFailed`] for a URL that is malformed, is not the
/// `ltk` scheme, names no route the app serves, or carries parameters that route
/// rejects.
pub fn parse_deep_link_url(raw_url: &str) -> AppResult<DeepLinkRequest> {
    let parsed = Url::parse(raw_url)
        .map_err(|e| AppError::ValidationFailed(format!("Malformed deep-link URL: {e}")))?;

    if parsed.scheme() != "ltk" {
        return Err(AppError::ValidationFailed(format!(
            "Expected 'ltk' scheme, got '{}'",
            parsed.scheme()
        )));
    }

    // The host portion of ltk://install is "install"
    let host = parsed.host_str().unwrap_or("");
    let path = parsed.path().trim_start_matches('/');
    let action = if !host.is_empty() { host } else { path };

    let pairs: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

    match action {
        "install" => parse_install(&pairs).map(DeepLinkRequest::Install),
        "settings" => parse_settings(&pairs).map(DeepLinkRequest::Settings),
        _ => Err(AppError::ValidationFailed(format!(
            "Unknown action '{action}', expected 'install' or 'settings'"
        ))),
    }
}

/// Longest `?focus=` value accepted, well past the longest id the app draws.
const FOCUS_MAX_CHARS: usize = 128;

fn parse_settings(pairs: &HashMap<String, String>) -> AppResult<DeepLinkSettingsRequest> {
    let focus = pairs
        .get("focus")
        .ok_or_else(|| AppError::ValidationFailed("Missing required 'focus' parameter".into()))?;

    if focus.is_empty() || focus.chars().count() > FOCUS_MAX_CHARS {
        return Err(AppError::ValidationFailed(format!(
            "'focus' must be 1 to {FOCUS_MAX_CHARS} characters"
        )));
    }

    // The value is handed to the frontend to put back into its own URL, and an id is
    // only ever a namespace, a dot and a name. An id this rejects is not one that
    // resolves, so nothing is lost by refusing it at the boundary.
    if !focus
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(AppError::ValidationFailed(
            "'focus' may only contain letters, digits, '.', '-' and '_'".into(),
        ));
    }

    Ok(DeepLinkSettingsRequest {
        focus: focus.clone(),
    })
}

fn parse_install(pairs: &HashMap<String, String>) -> AppResult<DeepLinkInstallRequest> {
    let download_url = pairs
        .get("url")
        .ok_or_else(|| AppError::ValidationFailed("Missing required 'url' parameter".into()))?;

    let download_parsed = Url::parse(download_url)
        .map_err(|e| AppError::ValidationFailed(format!("Invalid download URL: {e}")))?;

    if download_parsed.scheme() != "https" {
        return Err(AppError::ValidationFailed(
            "Download URL must use HTTPS".into(),
        ));
    }

    // SSRF prevention: reject loopback/private hosts
    if let Some(host) = download_parsed.host_str() {
        let lower = host.to_lowercase();
        let normalized = lower.trim_start_matches('[').trim_end_matches(']');
        if normalized == "localhost"
            || normalized == "127.0.0.1"
            || normalized == "::1"
            || normalized.starts_with("10.")
            || normalized.starts_with("192.168.")
            || normalized == "0.0.0.0"
        {
            return Err(AppError::ValidationFailed(
                "Download URL must not point to a local/private address".into(),
            ));
        }
        // Check 172.16.0.0/12 range
        if normalized.starts_with("172.") {
            if let Some(second_octet) = normalized
                .strip_prefix("172.")
                .and_then(|s| s.split('.').next())
                .and_then(|s| s.parse::<u8>().ok())
            {
                if (16..=31).contains(&second_octet) {
                    return Err(AppError::ValidationFailed(
                        "Download URL must not point to a local/private address".into(),
                    ));
                }
            }
        }
    }

    let name = pairs.get("name").map(|s| truncate_str(s, 256).to_string());
    let author = pairs
        .get("author")
        .map(|s| truncate_str(s, 256).to_string());
    let source = pairs
        .get("source")
        .map(|s| truncate_str(s, 256).to_string());

    Ok(DeepLinkInstallRequest {
        url: download_url.clone(),
        name,
        author,
        source,
        untrusted_domain: None,
    })
}

/// Emit a completion progress event.
pub fn emit_install_complete(app_handle: &tauri::AppHandle) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "protocol-install-progress",
        ProtocolInstallProgress {
            stage: "complete".to_string(),
            bytes_downloaded: 0,
            total_bytes: None,
            error: None,
        },
    );
}

/// Check if a hostname is trusted against a domain allowlist.
///
/// Matches exact domain or any subdomain (e.g., `cdn.runeforge.dev` matches `runeforge.dev`).
/// Returns `true` if `trusted_domains` is empty (allowlist disabled).
pub fn is_domain_trusted(download_url: &str, trusted_domains: &[String]) -> bool {
    if trusted_domains.is_empty() {
        return true;
    }

    let host = match Url::parse(download_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
    {
        Some(h) => h,
        None => return false,
    };

    trusted_domains
        .iter()
        .any(|d| host == d.as_str() || host.ends_with(&format!(".{d}")))
}

/// The host a download URL names, lower-cased, or `None` for a URL with no host.
pub fn download_host(download_url: &str) -> Option<String> {
    Url::parse(download_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
}

/// Process deep-link URLs and emit events to the frontend.
pub fn handle_urls(app_handle: &tauri::AppHandle, urls: &[url::Url]) {
    for url in urls {
        handle_single(app_handle, url.as_str());
    }
}

/// Process deep-link URLs from CLI argv (used by single-instance callback).
pub fn handle_argv(app_handle: &tauri::AppHandle, argv: &[String]) {
    for arg in argv.iter().skip(1) {
        if arg.starts_with("ltk://") {
            handle_single(app_handle, arg);
        }
    }

    crate::commands::shell::raise_main_window(app_handle);
}

fn handle_single(app_handle: &tauri::AppHandle, raw_url: &str) {
    tracing::info!("Received deep-link: {}", raw_url);

    let deep_link_state: tauri::State<'_, DeepLinkState> = app_handle.state();
    if deep_link_state.should_rate_limit() {
        tracing::warn!("Deep-link rate-limited, ignoring: {}", raw_url);
        return;
    }

    match parse_deep_link_url(raw_url) {
        Ok(mut request) => {
            tracing::info!("Parsed deep-link request: {:?}", request);

            if let DeepLinkRequest::Install(install) = &mut request {
                install.untrusted_domain = untrusted_domain(app_handle, install);
            }

            deep_link_state.deliver(app_handle, request);
        }
        Err(e) => {
            tracing::error!("Failed to parse deep-link URL: {}", e);
        }
    }
}

/// The host an install would download from, where the allowlist does not cover it.
///
/// The link still reaches the frontend, which asks the reader to trust the domain
/// or reject the install. Nothing downloads until they answer, because
/// `deep_link_install_mod` reads the same allowlist.
fn untrusted_domain(
    app_handle: &tauri::AppHandle,
    request: &DeepLinkInstallRequest,
) -> Option<String> {
    let settings_state: tauri::State<'_, SettingsState> = app_handle.state();
    let settings = settings_state.0.lock();

    if is_domain_trusted(&request.url, &settings.trusted_domains) {
        return None;
    }

    let domain = download_host(&request.url)?;
    tracing::warn!("Deep-link domain '{domain}' is not in the trusted list, asking the reader");
    Some(domain)
}

fn truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use fs_err as fs;

    /// The install route a raw URL names, for the tests that read its fields.
    fn install(raw_url: &str) -> AppResult<DeepLinkInstallRequest> {
        match parse_deep_link_url(raw_url)? {
            DeepLinkRequest::Install(request) => Ok(request),
            other => panic!("expected an install route, got {other:?}"),
        }
    }

    #[test]
    fn parse_valid_minimal_url() {
        let req = install("ltk://install?url=https://cdn.example.com/mods/skin.modpkg").unwrap();
        assert_eq!(req.url, "https://cdn.example.com/mods/skin.modpkg");
        assert!(req.name.is_none());
        assert!(req.source.is_none());
    }

    #[test]
    fn parse_valid_full_url() {
        let req = install(
            "ltk://install?url=https://cdn.example.com/mods/skin.modpkg&name=Cool%20Skin&author=SkinMaker&source=MySite",
        )
        .unwrap();
        assert_eq!(req.name.as_deref(), Some("Cool Skin"));
        assert_eq!(req.author.as_deref(), Some("SkinMaker"));
        assert_eq!(req.source.as_deref(), Some("MySite"));
    }

    #[test]
    fn rejects_http_url() {
        let err = parse_deep_link_url("ltk://install?url=http://cdn.example.com/mods/skin.modpkg");
        assert!(err.is_err());
    }

    #[test]
    fn rejects_missing_url_param() {
        let err = parse_deep_link_url("ltk://install?name=Test");
        assert!(err.is_err());
    }

    #[test]
    fn rejects_localhost() {
        let err = parse_deep_link_url("ltk://install?url=https://localhost/mods/skin.modpkg");
        assert!(err.is_err());
    }

    #[test]
    fn rejects_private_ip() {
        let err = parse_deep_link_url("ltk://install?url=https://192.168.1.1/mods/skin.modpkg");
        assert!(err.is_err());
    }

    #[test]
    fn ignores_unknown_params() {
        let req = install(
            "ltk://install?url=https://cdn.example.com/mods/skin.modpkg&checksum=sha256:abc&api=https://api.example.com&unknown=value",
        )
        .unwrap();
        assert_eq!(req.url, "https://cdn.example.com/mods/skin.modpkg");
    }

    #[test]
    fn rejects_wrong_scheme() {
        let err =
            parse_deep_link_url("https://install?url=https://cdn.example.com/mods/skin.modpkg");
        assert!(err.is_err());
    }

    #[test]
    fn rejects_unknown_action() {
        let err = parse_deep_link_url("ltk://update?url=https://cdn.example.com/mods/skin.modpkg");
        assert!(err.is_err());
    }

    // --- The settings route ---

    #[test]
    fn parse_settings_route() {
        let request = parse_deep_link_url("ltk://settings?focus=appearance.theme").unwrap();
        assert_eq!(
            request,
            DeepLinkRequest::Settings(DeepLinkSettingsRequest {
                focus: "appearance.theme".into(),
            })
        );
    }

    #[test]
    fn parse_settings_group_id() {
        let request = parse_deep_link_url("ltk://settings?focus=patching.mod-safety").unwrap();
        assert_eq!(
            request,
            DeepLinkRequest::Settings(DeepLinkSettingsRequest {
                focus: "patching.mod-safety".into(),
            })
        );
    }

    #[test]
    fn settings_ignores_unknown_params() {
        let request =
            parse_deep_link_url("ltk://settings?focus=general.autoRun&tab=appearance").unwrap();
        assert_eq!(
            request,
            DeepLinkRequest::Settings(DeepLinkSettingsRequest {
                focus: "general.autoRun".into(),
            })
        );
    }

    #[test]
    fn rejects_settings_without_focus() {
        assert!(parse_deep_link_url("ltk://settings").is_err());
        assert!(parse_deep_link_url("ltk://settings?tab=general").is_err());
    }

    #[test]
    fn rejects_empty_focus() {
        assert!(parse_deep_link_url("ltk://settings?focus=").is_err());
    }

    #[test]
    fn rejects_focus_outside_the_id_alphabet() {
        assert!(parse_deep_link_url("ltk://settings?focus=general.autoRun%20drop").is_err());
        assert!(parse_deep_link_url("ltk://settings?focus=%3Cscript%3E").is_err());
        assert!(parse_deep_link_url("ltk://settings?focus=a%2Fb").is_err());
    }

    #[test]
    fn rejects_focus_over_the_limit() {
        let long = "a".repeat(FOCUS_MAX_CHARS + 1);
        assert!(parse_deep_link_url(&format!("ltk://settings?focus={long}")).is_err());

        let longest = "a".repeat(FOCUS_MAX_CHARS);
        assert!(parse_deep_link_url(&format!("ltk://settings?focus={longest}")).is_ok());
    }

    // --- The hand-off ---

    #[test]
    fn holds_a_link_that_arrives_before_the_frontend_listens() {
        let state = DeepLinkState::new();
        let request = DeepLinkRequest::Settings(DeepLinkSettingsRequest {
            focus: "appearance.theme".into(),
        });

        state.handoff.lock().pending = Some(request.clone());

        assert_eq!(state.take_pending(), Some(request));
        assert_eq!(state.take_pending(), None);
    }

    #[test]
    fn take_pending_marks_the_frontend_listening() {
        let state = DeepLinkState::new();
        assert!(!state.handoff.lock().listening);

        state.take_pending();

        assert!(state.handoff.lock().listening);
    }

    #[test]
    fn rate_limiter_blocks_rapid_calls() {
        let state = DeepLinkState::new();
        assert!(!state.should_rate_limit());
        assert!(state.should_rate_limit());
    }

    // --- URL parsing: additional edge cases ---

    #[test]
    fn parse_url_encoded_params() {
        let req = install(
            "ltk://install?url=https://cdn.example.com/mod.modpkg&name=%E2%9C%A8%20Sparkle%20Skin&source=My%20Site",
        )
        .unwrap();
        assert_eq!(req.name.as_deref(), Some("✨ Sparkle Skin"));
        assert_eq!(req.source.as_deref(), Some("My Site"));
    }

    #[test]
    fn parse_empty_optional_params() {
        let req =
            install("ltk://install?url=https://cdn.example.com/mod.modpkg&name=&source=").unwrap();
        assert_eq!(req.name.as_deref(), Some(""));
        assert_eq!(req.source.as_deref(), Some(""));
    }

    #[test]
    fn parse_url_with_query_string_in_download_url() {
        let req = install("ltk://install?url=https://cdn.example.com/mod.modpkg%3Ftoken%3Dabc123")
            .unwrap();
        assert!(req.url.contains("token"));
    }

    #[test]
    fn truncates_long_name_at_256_chars() {
        let long_name: String = "あ".repeat(300);
        let url = format!(
            "ltk://install?url=https://cdn.example.com/mod.modpkg&name={}",
            long_name
        );
        let req = install(&url).unwrap();
        assert_eq!(req.name.as_ref().unwrap().chars().count(), 256);
    }

    #[test]
    fn rejects_completely_malformed_url() {
        assert!(parse_deep_link_url("not a url at all").is_err());
    }

    #[test]
    fn rejects_empty_string() {
        assert!(parse_deep_link_url("").is_err());
    }

    // --- SSRF prevention ---

    #[test]
    fn rejects_loopback_127() {
        assert!(parse_deep_link_url("ltk://install?url=https://127.0.0.1/mod.modpkg").is_err());
    }

    #[test]
    fn rejects_ipv6_loopback() {
        assert!(parse_deep_link_url("ltk://install?url=https://[::1]/mod.modpkg").is_err());
    }

    #[test]
    fn rejects_10_x_private_range() {
        assert!(parse_deep_link_url("ltk://install?url=https://10.0.0.1/mod.modpkg").is_err());
        assert!(
            parse_deep_link_url("ltk://install?url=https://10.255.255.255/mod.modpkg").is_err()
        );
    }

    #[test]
    fn rejects_172_16_to_31_private_range() {
        assert!(parse_deep_link_url("ltk://install?url=https://172.16.0.1/mod.modpkg").is_err());
        assert!(
            parse_deep_link_url("ltk://install?url=https://172.31.255.255/mod.modpkg").is_err()
        );
    }

    #[test]
    fn allows_172_outside_private_range() {
        assert!(parse_deep_link_url("ltk://install?url=https://172.15.0.1/mod.modpkg").is_ok());
        assert!(parse_deep_link_url("ltk://install?url=https://172.32.0.1/mod.modpkg").is_ok());
    }

    #[test]
    fn rejects_0000_address() {
        assert!(parse_deep_link_url("ltk://install?url=https://0.0.0.0/mod.modpkg").is_err());
    }

    #[test]
    fn rejects_localhost_uppercase() {
        assert!(parse_deep_link_url("ltk://install?url=https://LOCALHOST/mod.modpkg").is_err());
    }

    // --- Domain allowlist validation ---

    #[test]
    fn domain_trusted_empty_allowlist_allows_all() {
        assert!(is_domain_trusted("https://anything.com/mod.modpkg", &[]));
    }

    #[test]
    fn domain_trusted_exact_match() {
        let domains = vec!["runeforge.dev".into()];
        assert!(is_domain_trusted(
            "https://runeforge.dev/mod.modpkg",
            &domains
        ));
    }

    #[test]
    fn domain_trusted_subdomain_match() {
        let domains = vec!["runeforge.dev".into()];
        assert!(is_domain_trusted(
            "https://cdn.runeforge.dev/mod.modpkg",
            &domains
        ));
    }

    #[test]
    fn domain_trusted_deep_subdomain_match() {
        let domains = vec!["runeforge.dev".into()];
        assert!(is_domain_trusted(
            "https://files.cdn.runeforge.dev/mod.modpkg",
            &domains
        ));
    }

    #[test]
    fn download_host_is_lower_cased() {
        assert_eq!(
            download_host("https://CDN.RuneForge.Dev/mod.modpkg").as_deref(),
            Some("cdn.runeforge.dev")
        );
    }

    #[test]
    fn download_host_of_a_hostless_url() {
        assert_eq!(download_host("not a url"), None);
    }

    /// Trust is the reader's setting, so parsing alone never claims a verdict.
    #[test]
    fn parsing_leaves_the_trust_unstamped() {
        let req = install("ltk://install?url=https://cdn.example.com/mods/skin.modpkg").unwrap();
        assert_eq!(req.untrusted_domain, None);
    }

    #[test]
    fn domain_trusted_rejects_partial_suffix() {
        let domains = vec!["runeforge.dev".into()];
        assert!(!is_domain_trusted(
            "https://evilruneforge.dev/mod.modpkg",
            &domains
        ));
    }

    #[test]
    fn domain_trusted_rejects_unlisted_domain() {
        let domains = vec!["runeforge.dev".into()];
        assert!(!is_domain_trusted("https://evil.com/mod.modpkg", &domains));
    }

    #[test]
    fn domain_trusted_case_insensitive() {
        let domains = vec!["runeforge.dev".into()];
        assert!(is_domain_trusted(
            "https://RUNEFORGE.DEV/mod.modpkg",
            &domains
        ));
        assert!(is_domain_trusted(
            "https://CDN.RuneForge.Dev/mod.modpkg",
            &domains
        ));
    }

    #[test]
    fn domain_trusted_multiple_domains() {
        let domains = vec!["runeforge.dev".into(), "divineskins.gg".into()];
        assert!(is_domain_trusted(
            "https://runeforge.dev/mod.modpkg",
            &domains
        ));
        assert!(is_domain_trusted(
            "https://divineskins.gg/mod.modpkg",
            &domains
        ));
        assert!(!is_domain_trusted("https://evil.com/mod.modpkg", &domains));
    }

    #[test]
    fn domain_trusted_invalid_url_returns_false() {
        let domains = vec!["runeforge.dev".into()];
        assert!(!is_domain_trusted("not a url", &domains));
    }

    // --- Content-Disposition parsing ---

    #[test]
    fn content_disposition_modpkg_filename() {
        assert_eq!(
            extract_extension_from_content_disposition(
                r#"attachment; filename="cool-skin.modpkg""#
            ),
            Some("modpkg")
        );
    }

    #[test]
    fn content_disposition_fantome_filename() {
        assert_eq!(
            extract_extension_from_content_disposition(
                r#"attachment; filename="cool-skin.fantome""#
            ),
            Some("fantome")
        );
    }

    #[test]
    fn content_disposition_no_filename() {
        assert_eq!(
            extract_extension_from_content_disposition("attachment"),
            None
        );
    }

    #[test]
    fn content_disposition_unknown_extension() {
        assert_eq!(
            extract_extension_from_content_disposition(r#"attachment; filename="archive.zip""#),
            None
        );
    }

    #[test]
    fn content_disposition_case_insensitive() {
        assert_eq!(
            extract_extension_from_content_disposition(r#"Attachment; Filename="skin.MODPKG""#),
            Some("modpkg")
        );
    }

    #[test]
    fn content_disposition_filename_star_utf8() {
        assert_eq!(
            extract_extension_from_content_disposition(
                "attachment; filename*=UTF-8''cool%20skin.fantome"
            ),
            Some("fantome")
        );
    }

    // --- File magic sniffing ---

    #[test]
    fn sniff_zip_magic_returns_fantome() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_sniff_zip.tmp");
        fs::write(&path, [0x50, 0x4B, 0x03, 0x04, 0x00, 0x00]).unwrap();
        assert_eq!(sniff_extension_from_file(&path), Some("fantome".into()));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn sniff_non_zip_returns_none() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_sniff_nonzip.tmp");
        fs::write(&path, b"not a zip file at all").unwrap();
        assert_eq!(sniff_extension_from_file(&path), None);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn sniff_empty_file_returns_none() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_sniff_empty.tmp");
        fs::write(&path, b"").unwrap();
        assert_eq!(sniff_extension_from_file(&path), None);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn sniff_short_file_returns_none() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_sniff_short.tmp");
        fs::write(&path, [0x50, 0x4B]).unwrap();
        assert_eq!(sniff_extension_from_file(&path), None);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn sniff_nonexistent_file_returns_none() {
        let path = std::env::temp_dir().join("test_sniff_nonexistent_file_that_does_not_exist.tmp");
        assert_eq!(sniff_extension_from_file(&path), None);
    }
}
