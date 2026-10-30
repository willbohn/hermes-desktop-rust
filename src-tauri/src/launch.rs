//! Bounded launcher for the local `hermes webapp` CLI.
//!
//! The launcher runs the pinned upstream `hermes webapp` command in a child process, captures its
//! output under a hard byte cap and a deadline, and extracts the one-use launch link the CLI
//! prints. It owns only the process it starts: a timeout, a cancellation or an output overflow
//! kills and reaps that direct child and never a detached gateway.
//!
//! A launch link's fragment is a single-use credential, so this module never logs or returns raw
//! output. Diagnostics are fixed, credential-free messages: a nonzero exit is classified against
//! the exact refusals the pinned CLI prints, and anything unrecognized becomes a generic message
//! with the exit status. [`LaunchTarget`]'s `Debug` prints only the query- and fragment-free base.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use serde::Serialize;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::time::{Instant, sleep_until};
use url::{Host, Url};

/// Deadline for [`open_local`]: the gateway allows 60 s plus its own startup.
pub const DEFAULT_OPEN_DEADLINE: Duration = Duration::from_secs(90);
/// Deadline for [`build_renderer`]: a cold renderer build takes minutes.
pub const DEFAULT_BUILD_DEADLINE: Duration = Duration::from_secs(15 * 60);
/// Most bytes captured from each of the child's output streams before it is stopped.
pub const DEFAULT_OUTPUT_LIMIT: usize = 1 << 20;
/// How long the launcher keeps draining a stream after the child has exited.
///
/// A detached descendant of the CLI can inherit the CLI's stdout/stderr and hold the write end of
/// those pipes open after the CLI itself exits, so EOF never arrives. Once the child has exited the
/// launcher drains whatever it flushed for this grace period and then proceeds; the hard deadline
/// still bounds a child that never exits at all.
const POST_EXIT_DRAIN: Duration = Duration::from_millis(250);
/// Longest accepted profile name; mirrors upstream `PROFILE_ID_RE` (`^[a-z0-9][a-z0-9_-]{0,63}$`).
const MAX_PROFILE_LEN: usize = 64;

/// Inputs to a local launch or renderer build.
#[derive(Clone)]
pub struct LocalOptions {
    /// The CLI executable; defaults to `hermes`.
    pub program: PathBuf,
    /// Named profile passed as `-p <profile>`; validated as an upstream profile slug.
    pub profile: Option<String>,
    /// Explicit `HERMES_HOME` for the child; `None` inherits the caller's environment.
    pub home: Option<PathBuf>,
    /// Explicit `HERMES_WEBAPP_DIST` for the child; `None` inherits the caller's environment.
    pub renderer: Option<PathBuf>,
}

impl Default for LocalOptions {
    fn default() -> Self {
        Self {
            program: PathBuf::from("hermes"),
            profile: None,
            home: None,
            renderer: None,
        }
    }
}

/// A validated Webapp destination.
pub struct LaunchTarget {
    url: Url,
}

impl LaunchTarget {
    /// The full launch URL, including any single-use fragment credential. Only use this where the
    /// link is actually handed to a browser.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// The URL without its query or fragment, safe to show or log.
    pub fn safe_label(&self) -> String {
        let mut base = self.url.clone();
        base.set_query(None);
        base.set_fragment(None);
        base.to_string()
    }
}

impl std::fmt::Debug for LaunchTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchTarget")
            .field("url", &self.safe_label())
            .finish()
    }
}

/// A launch or build failure. `message` and `recovery` are actionable and carry no credentials.
#[derive(Debug, Error, Serialize)]
#[error("{message}")]
pub struct LaunchError {
    /// Stable machine-readable kind.
    pub code: String,
    /// What went wrong, safe to show the user.
    pub message: String,
    /// What the user can do next, safe to show the user.
    pub recovery: String,
}

impl LaunchError {
    fn new(code: &str, message: impl Into<String>, recovery: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recovery: recovery.into(),
        }
    }
}

/// Open the Webapp for `options`, returning the one-use launch link its gateway minted.
pub async fn open_local(options: &LocalOptions) -> Result<LaunchTarget, LaunchError> {
    open_local_bounded(options, DEFAULT_OPEN_DEADLINE, DEFAULT_OUTPUT_LIMIT).await
}

/// [`open_local`] with an explicit deadline and per-stream output cap, for callers that need a
/// shorter bound than [`DEFAULT_OPEN_DEADLINE`].
pub async fn open_local_bounded(
    options: &LocalOptions,
    deadline: Duration,
    output_limit: usize,
) -> Result<LaunchTarget, LaunchError> {
    if let Some(profile) = &options.profile {
        validate_profile(profile)?;
    }
    let argv = open_argv(options);
    let captured = run(options, &argv, deadline, output_limit).await?;
    if !captured.status.success() {
        return Err(exit_failure(&captured));
    }
    // A link is only trusted from a successful exit; a failing run's output is never parsed.
    let stdout = std::str::from_utf8(&captured.stdout).map_err(|_| unreadable_output())?;
    let url = parse_launch_link(stdout)?;
    Ok(LaunchTarget { url })
}

/// Build the Webapp renderer bundle (`hermes webapp --build-only`).
pub async fn build_renderer(options: &LocalOptions) -> Result<(), LaunchError> {
    build_renderer_bounded(options, DEFAULT_BUILD_DEADLINE, DEFAULT_OUTPUT_LIMIT).await
}

/// [`build_renderer`] with an explicit deadline and per-stream output cap.
pub async fn build_renderer_bounded(
    options: &LocalOptions,
    deadline: Duration,
    output_limit: usize,
) -> Result<(), LaunchError> {
    if let Some(profile) = &options.profile {
        validate_profile(profile)?;
    }
    let argv = build_argv();
    let captured = run(options, &argv, deadline, output_limit).await?;
    if !captured.status.success() {
        return Err(exit_failure(&captured));
    }
    Ok(())
}

/// The argv after the program: `[-p <profile>] webapp --no-open --skip-build`.
fn open_argv(options: &LocalOptions) -> Vec<OsString> {
    let mut argv = Vec::new();
    if let Some(profile) = &options.profile {
        argv.push(OsString::from("-p"));
        argv.push(OsString::from(profile));
    }
    argv.push(OsString::from("webapp"));
    argv.push(OsString::from("--no-open"));
    argv.push(OsString::from("--skip-build"));
    argv
}

/// The argv after the program: `webapp --build-only`.
fn build_argv() -> Vec<OsString> {
    vec![OsString::from("webapp"), OsString::from("--build-only")]
}

/// A completed child, captured under the output cap.
struct Captured {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Why a bounded run stopped before the child finished on its own.
enum Failure {
    Timeout,
    Overflow,
    Io,
}

impl Failure {
    /// The fixed error for a run that stopped short, carrying no child output.
    fn into_error(self, deadline: Duration) -> LaunchError {
        match self {
            Failure::Timeout => LaunchError::new(
                "timeout",
                format!(
                    "The Hermes CLI did not finish within {} seconds.",
                    deadline.as_secs()
                ),
                "Check that the gateway can start, then try again.",
            ),
            Failure::Overflow => LaunchError::new(
                "output_overflow",
                "The Hermes CLI printed more output than the launcher accepts.",
                "Update the Hermes CLI, then try again.",
            ),
            Failure::Io => io_error(),
        }
    }
}

/// Run `options.program` with `argv` under a deadline and an output cap, never touching any process
/// it did not start.
///
/// The two pipes are read inline in one `select!` loop, so no reader task is ever left detached:
/// dropping the readers here closes their descriptors deterministically. Once the child exits the
/// loop stops waiting for EOF after [`POST_EXIT_DRAIN`], so a descendant that inherits the pipes
/// cannot turn a successful launch into a false timeout.
async fn run(
    options: &LocalOptions,
    argv: &[OsString],
    deadline: Duration,
    output_limit: usize,
) -> Result<Captured, LaunchError> {
    let mut command = Command::new(&options.program);
    command
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(home) = &options.home {
        command.env("HERMES_HOME", home);
    }
    if let Some(renderer) = &options.renderer {
        command.env("HERMES_WEBAPP_DIST", renderer);
    }

    let mut child = command.spawn().map_err(|_| {
        LaunchError::new(
            "spawn_failed",
            format!(
                "Could not start the Hermes CLI at {}.",
                options.program.display()
            ),
            "Install the Hermes CLI, put it on PATH, or point `program` at it.",
        )
    })?;

    let mut stdout = child.stdout.take().ok_or_else(io_error)?;
    let mut stderr = child.stderr.take().ok_or_else(io_error)?;
    let mut out_bytes = Vec::new();
    let mut err_bytes = Vec::new();
    let mut out_chunk = [0u8; 8192];
    let mut err_chunk = [0u8; 8192];
    let mut out_eof = false;
    let mut err_eof = false;
    let mut status: Option<ExitStatus> = None;
    let mut drain_until: Option<Instant> = None;
    let mut failure = None;

    let deadline_at = Instant::now() + deadline;
    loop {
        if out_eof && err_eof && status.is_some() {
            break;
        }
        // Once the child has exited, stop waiting for EOF at the drain deadline; the hard deadline
        // still bounds a child that never exits.
        let wake_at = drain_until.map_or(deadline_at, |at| at.min(deadline_at));
        tokio::select! {
            biased;
            read = stdout.read(&mut out_chunk), if !out_eof => match read {
                Ok(0) => out_eof = true,
                Ok(read) => {
                    if out_bytes.len() + read > output_limit {
                        failure = Some(Failure::Overflow);
                        break;
                    }
                    out_bytes.extend_from_slice(&out_chunk[..read]);
                }
                Err(_) => { failure = Some(Failure::Io); break; }
            },
            read = stderr.read(&mut err_chunk), if !err_eof => match read {
                Ok(0) => err_eof = true,
                Ok(read) => {
                    if err_bytes.len() + read > output_limit {
                        failure = Some(Failure::Overflow);
                        break;
                    }
                    err_bytes.extend_from_slice(&err_chunk[..read]);
                }
                Err(_) => { failure = Some(Failure::Io); break; }
            },
            result = child.wait(), if status.is_none() => match result {
                Ok(exit) => {
                    status = Some(exit);
                    drain_until = Some(Instant::now() + POST_EXIT_DRAIN);
                }
                Err(_) => { failure = Some(Failure::Io); break; }
            },
            () = sleep_until(wake_at) => {
                // The child has exited, so this is the drain deadline, not a timeout.
                if status.is_none() {
                    failure = Some(Failure::Timeout);
                }
                break;
            }
        }
    }

    if let Some(failure) = failure {
        terminate(&mut child).await;
        return Err(failure.into_error(deadline));
    }

    Ok(Captured {
        status: status.expect("the loop ends without a failure only after the child exited"),
        stdout: out_bytes,
        stderr: err_bytes,
    })
}

/// Terminate and reap the child this launcher started; never a detached gateway.
async fn terminate(child: &mut Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

fn io_error() -> LaunchError {
    LaunchError::new(
        "io_failed",
        "The launcher could not read the Hermes CLI process.",
        "Try again; if it persists, run `hermes webapp --no-open --skip-build` yourself.",
    )
}

fn unreadable_output() -> LaunchError {
    LaunchError::new(
        "bad_output",
        "The Hermes CLI printed output the launcher could not read.",
        "Update the Hermes CLI, then try again.",
    )
}

fn no_launch_link() -> LaunchError {
    LaunchError::new(
        "bad_output",
        "The Hermes CLI did not print a Webapp launch link.",
        "Update the Hermes CLI, then try again; run `hermes webapp --no-open` yourself to see its output.",
    )
}

/// A non-zero exit, reported as a fixed message that never carries the child's output.
///
/// A known refusal gets its own code and recovery; everything else is generic and names only the
/// exit status. The raw stdout/stderr is read to classify the failure and then discarded.
fn exit_failure(captured: &Captured) -> LaunchError {
    if let Some(error) = known_refusal(captured) {
        return error;
    }
    let code = captured
        .status
        .code()
        .map_or_else(|| "signal".to_string(), |code| code.to_string());
    LaunchError::new(
        "exit_failure",
        format!("The Hermes CLI exited with status {code}."),
        "Run `hermes webapp --no-open --skip-build` yourself to see the full error.",
    )
}

/// Classify a non-zero exit against the fixed refusals the pinned CLI prints
/// (`hermes_cli/webapp_launcher.py`, `gateway/webapp_listener.py`, `gateway/host_attach.py`).
///
/// The CLI prints most refusals to stdout (`print`) and the deleted-profile refusal to stderr, so
/// both streams are inspected. Only a known prefix decides the message; no captured text is ever
/// copied into the returned error.
fn known_refusal(captured: &Captured) -> Option<LaunchError> {
    let stdout = String::from_utf8_lossy(&captured.stdout);
    let stderr = String::from_utf8_lossy(&captured.stderr);
    let streams = [stdout.as_ref(), stderr.as_ref()];
    let mentions = |needle: &str| streams.iter().any(|stream| stream.contains(needle));

    // `webapp_launcher._off_message`: the home's config keeps the Webapp off.
    if mentions("The Webapp is off") {
        return Some(LaunchError::new(
            "webapp_off",
            "The Webapp is turned off for this Hermes home.",
            "Set `webapp.enabled: true` in this home's `config.yaml`, then run `hermes webapp` again.",
        ));
    }
    // `webapp_launcher.STARTED_BEFORE_ON`: the running gateway predates the Webapp being turned on.
    if mentions("The gateway started before the Webapp was turned on") {
        return Some(LaunchError::new(
            "gateway_stale",
            "The running gateway started before the Webapp was turned on, so it does not serve it.",
            "Restart the gateway with `hermes gateway restart`, then try again.",
        ));
    }
    // `webapp_listener.NOT_BUILT`: the renderer bundle is missing.
    if mentions("The Webapp is not built") {
        return Some(LaunchError::new(
            "renderer_missing",
            "The Webapp renderer bundle is not built.",
            "Build it with `hermes webapp --build-only`, then try again.",
        ));
    }
    // `host_attach._refuse_message` / `_unknown_served_message`: another gateway owns this host.
    if mentions("A gateway already owns this host") {
        return Some(LaunchError::new(
            "host_owner_conflict",
            "Another Hermes gateway already owns this host and will not serve this profile.",
            "Restart or migrate that gateway, or launch the profile it serves; do not start a second gateway.",
        ));
    }
    None
}

/// The prefix of the sign-in launch message; the link follows it on the same line.
const SIGN_IN_PREFIX: &str = "Open the Webapp and sign in with your dashboard account: ";
/// The prefix of the private-link preamble; the link is the next line.
const PRIVATE_PREAMBLE_PREFIX: &str = "Open this link in your browser within ";
const PRIVATE_PREAMBLE_SUFFIX: &str = " so keep it private:";
/// The prefix of the one-use-link preamble; the link is the next line.
const ONE_USE_PREAMBLE_PREFIX: &str = "Open the Webapp with this one-use link (valid for ";
const ONE_USE_PREAMBLE_SUFFIX: &str = " seconds):";

/// Extract the launch link from a successful `hermes webapp --no-open --skip-build` run.
///
/// The link is read only from the exact messages the pinned CLI prints
/// (`hermes_cli/webapp_launcher.py`): the inline sign-in message, or the private/one-use preamble
/// whose following line is the indented link. No other token is considered, so a banner, a help
/// URL or a note printed around the message can never be mistaken for the workspace. When more
/// than one message yields a link, a one-use `/webapp/launch#ticket=` link is preferred.
fn parse_launch_link(stdout: &str) -> Result<Url, LaunchError> {
    let mut candidates: Vec<Url> = Vec::new();
    let mut lines = stdout.lines();
    while let Some(line) = lines.next() {
        let line = line.trim_end_matches('\r');
        if let Some(url) = line.strip_prefix(SIGN_IN_PREFIX) {
            push_candidate(&mut candidates, url);
        } else if is_link_preamble(line)
            && let Some(url) = lines.next()
        {
            push_candidate(&mut candidates, url);
        }
    }
    candidates
        .iter()
        .find(|url| has_ticket_fragment(url))
        .or_else(|| candidates.first())
        .cloned()
        .ok_or_else(no_launch_link)
}

/// Whether `line` is one of the exact preambles the pinned CLI prints before an indented link.
fn is_link_preamble(line: &str) -> bool {
    (line.starts_with(PRIVATE_PREAMBLE_PREFIX) && line.ends_with(PRIVATE_PREAMBLE_SUFFIX))
        || (line.starts_with(ONE_USE_PREAMBLE_PREFIX) && line.ends_with(ONE_USE_PREAMBLE_SUFFIX))
}

/// Accept `raw` as a candidate only if it is a whole URL over an allowed transport with no userinfo.
///
/// The candidate is taken verbatim from the message: no punctuation is trimmed, so a URL the CLI
/// prints with a stray delimiter is rejected rather than silently "repaired".
fn push_candidate(candidates: &mut Vec<Url>, raw: &str) {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().any(char::is_control) {
        return;
    }
    let Ok(url) = Url::parse(raw) else {
        return;
    };
    if url.username().is_empty() && url.password().is_none() && is_allowed_transport(&url) {
        candidates.push(url);
    }
}

/// `.../webapp/launch#ticket=<credential>` (optionally with other fragment parameters).
fn has_ticket_fragment(url: &Url) -> bool {
    let Some(fragment) = url.fragment() else {
        return false;
    };
    url.path().ends_with("/webapp/launch")
        && fragment.split('&').any(|part| {
            part.split_once('=')
                .is_some_and(|(key, value)| key == "ticket" && !value.is_empty())
        })
}

/// HTTPS everywhere, or plain HTTP only on loopback.
fn is_allowed_transport(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => is_loopback(url),
        _ => false,
    }
}

/// Loopback, as the pinned CLI itself counts it (`webapp_launcher._plain_http_note`): `localhost`,
/// `*.localhost`, `127.0.0.0/8` and `::1`.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        Some(Host::Domain(name)) => name == "localhost" || name.ends_with(".localhost"),
        None => false,
    }
}

fn validate_profile(profile: &str) -> Result<(), LaunchError> {
    if is_profile_slug(profile) {
        return Ok(());
    }
    Err(LaunchError::new(
        "invalid_profile",
        "The profile name is not a valid Hermes profile name.",
        "Use up to 64 lowercase letters, digits, underscores or hyphens, starting with a letter or digit.",
    ))
}

fn is_profile_slug(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_PROFILE_LEN
        && value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}
