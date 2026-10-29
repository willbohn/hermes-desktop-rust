//! Validated workspace destinations. A caller cannot opt into plaintext remote auth.
use std::path::{Path, PathBuf};
use url::{Host, Url};

pub fn is_launcher(label: &str, url: &Url) -> bool {
    label == "launcher"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        // Tauri omits index.html (and the slash on its custom scheme).
        && matches!(url.path(), "" | "/" | "/index.html")
        && matches!(
            (url.scheme(), url.host_str()),
            ("tauri", Some("localhost"))
                | ("http" | "https", Some("tauri.localhost"))
        )
}

pub fn navigation_allowed(workspace: &Url, target: &Url) -> bool {
    if target.as_str() == "about:blank" {
        return true;
    }
    if !target.username().is_empty() || target.password().is_some() {
        return false;
    }
    target.scheme() == "https"
        || (matches!(target.scheme(), "http" | "blob") && target.origin() == workspace.origin())
}

/// A `window.open` popup is handed to a new in-process workspace window only when it targets the
/// same origin as the document the user is *currently* on. The comparison uses the live document,
/// not the address the window first opened at, so a sign-in redirect (for example to an identity
/// provider) keeps its own same-origin popups. Any other popup is denied and offered to the system
/// browser instead; it never reaches a hosted page with native reach.
pub fn popup_origin_allowed(document: &Url, target: &Url) -> bool {
    matches!(target.scheme(), "http" | "https")
        && target.username().is_empty()
        && target.password().is_none()
        && document.origin().is_tuple()
        && target.origin() == document.origin()
}

/// The address a confirmation dialog may name: scheme, host and path only. The query and fragment
/// are dropped so a remote page cannot make the dialog echo a token it chose.
pub fn confirmable_link(url: &Url) -> String {
    let mut shown = url.clone();
    shown.set_query(None);
    shown.set_fragment(None);
    shown.to_string()
}

pub fn window_title(url: &Url) -> String {
    let origin = url.origin();
    let label = if origin.is_tuple() {
        origin.ascii_serialization()
    } else {
        "Workspace".to_owned()
    };
    format!("Hermes Desktop Rust — {label}")
}

pub fn remote_workspace_url(input: &str) -> Result<Url, &'static str> {
    if input.trim() != input || input.chars().any(char::is_control) {
        return Err("Enter an address without whitespace or control characters");
    }
    let url = Url::parse(input).map_err(|_| "Enter a complete https:// address")?;
    let loopback = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        Some(Host::Domain(name)) => name == "localhost",
        None => false,
    };
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err("Use HTTPS for remote workspaces; HTTP is allowed only on loopback");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Sign in inside the workspace; never put credentials in its address");
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Enter the workspace address without query parameters or a fragment");
    }
    Ok(url)
}

/// Names Windows refuses to create, matched case-insensitively on the stem.
const RESERVED_WINDOWS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Reduce a server-suggested download name to a single safe path segment. Directory separators,
/// control characters, characters no mainstream filesystem accepts, leading/trailing dots and
/// spaces are removed, the length is bounded, and a name that becomes empty or reserved falls back
/// to `download`. A remote page therefore never steers the directory and cannot pick a traversal
/// or device name.
pub fn sanitized_download_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        .collect();
    // Filesystems bound component bytes, not Unicode character count.
    let mut end = cleaned.len().min(180);
    while !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    let bounded = &cleaned[..end];
    let name = bounded.trim().trim_matches('.').trim();
    if name.is_empty() {
        return "download".to_owned();
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if RESERVED_WINDOWS_NAMES.contains(&stem.as_str()) {
        return format!("_{name}");
    }
    name.to_owned()
}

/// The only path a download may be written to: `dir` joined with a sanitized single segment.
pub fn download_destination(dir: &Path, suggested: &str) -> PathBuf {
    dir.join(sanitized_download_name(suggested))
}

/// Avoid existing files, directories and dangling symlinks in the user-approved directory.
/// The webview remains responsible for the transfer and its no-overwrite behavior.
pub fn next_download_destination(dir: &Path, suggested: &str) -> std::io::Result<PathBuf> {
    use std::io::{Error, ErrorKind};
    if !dir.is_dir() {
        return Err(Error::new(
            ErrorKind::NotFound,
            "Download folder is unavailable",
        ));
    }
    let original = download_destination(dir, suggested);
    let name = Path::new(original.file_name().expect("sanitized name"));
    let stem = name
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let extension = name
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    for index in 0..1000 {
        let candidate = if index == 0 {
            original.clone()
        } else {
            dir.join(format!("{stem} ({index}){extension}"))
        };
        match candidate.symlink_metadata() {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error),
            Ok(_) => (),
        }
    }
    Err(Error::new(
        ErrorKind::AlreadyExists,
        "Too many downloads with this name",
    ))
}

/// Browser-style zoom ladder. A menu action is always one bounded step, so a window cannot be
/// scaled into an unusable or invisible state.
pub const ZOOM_LEVELS: [f64; 12] = [
    0.5, 0.67, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0,
];

/// The next zoom level: the first ladder entry above `current` when `direction > 0`, the first below
/// it when `direction < 0`, and exactly 1.0 when `direction == 0`. The result is always a member of
/// [`ZOOM_LEVELS`], so repeated zooming is bounded at both ends and never returns the same level.
pub fn zoom_step(current: f64, direction: i8) -> f64 {
    if direction == 0 {
        return 1.0;
    }
    if direction > 0 {
        ZOOM_LEVELS
            .iter()
            .copied()
            .find(|level| *level > current)
            .unwrap_or(ZOOM_LEVELS[ZOOM_LEVELS.len() - 1])
    } else {
        ZOOM_LEVELS
            .iter()
            .copied()
            .rev()
            .find(|level| *level < current)
            .unwrap_or(ZOOM_LEVELS[0])
    }
}
