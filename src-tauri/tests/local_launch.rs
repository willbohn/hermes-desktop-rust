//! Behaviour tests for the local launcher.
//!
//! Every executable here is a throwaway shell script written by the test: it stands in for the
//! `hermes webapp` CLI and is never a live gateway, profile or model run. These tests prove the
//! launcher's process, bound and parsing contracts only; they are not evidence that a real
//! `hermes webapp` launch or agent turn works.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use hermes_desktop_rust::launch::{
    LaunchError, LaunchTarget, LocalOptions, build_renderer, build_renderer_bounded, open_local,
    open_local_bounded,
};
use tokio::sync::Mutex;

/// Shaped like the upstream `secrets.token_urlsafe(32)` ticket: 43 URL-safe base64 characters.
const TICKET: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";

/// Serializes fixture creation and execution across the async tests.
///
/// Writing a script and immediately exec'ing it races a *parallel* test's `fork`: the forked child
/// can inherit the script's still-open write descriptor, and the exec then fails with `ETXTBSY`
/// ("Text file busy"). Holding this lock across both the write and the run removes the race
/// deterministically. The launcher itself is never retried: a real host execs the installed
/// `hermes`, never a just-written file.
fn fixtures() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Write an executable `/bin/sh` script and return its path.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fixture");
    let mut permissions = std::fs::metadata(&path)
        .expect("stat fixture")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod fixture");
    path
}

/// Quote `text` as one POSIX single-quoted shell word.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// A shell `printf` that prints `lines`, one per line, to stdout.
fn print_lines(lines: &[&str]) -> String {
    let mut command = String::from("printf '%s\\n'");
    for line in lines {
        command.push(' ');
        command.push_str(&quote(line));
    }
    command
}

/// The exact multi-line message `hermes webapp --no-open` prints for a private one-use link
/// (`hermes_cli/webapp_launcher.py`, the `link.get("private")` branch).
fn private_message(url: &str) -> String {
    print_lines(&[
        "Open this link in your browser within 60 seconds. It works once and signs the browser in as you, so keep it private:",
        &format!("  {url}"),
    ])
}

/// The exact one-line message for a sign-in-gated Webapp.
fn sign_in_message(url: &str) -> String {
    print_lines(&[&format!(
        "Open the Webapp and sign in with your dashboard account: {url}"
    )])
}

/// The exact multi-line message for a non-private one-use link.
fn one_use_message(url: &str) -> String {
    print_lines(&[
        "Open the Webapp with this one-use link (valid for 60 seconds):",
        &format!("  {url}"),
    ])
}

fn link() -> String {
    "http://127.0.0.1:9120/webapp/launch#ticket=".to_string() + TICKET
}

fn options(program: PathBuf) -> LocalOptions {
    LocalOptions {
        program,
        ..Default::default()
    }
}

#[test]
fn defaults_target_the_hermes_cli_with_no_overrides() {
    let options = LocalOptions::default();
    assert_eq!(options.program, PathBuf::from("hermes"));
    assert!(options.profile.is_none());
    assert!(options.home.is_none());
    assert!(options.renderer.is_none());
    assert_eq!(options.clone().program, PathBuf::from("hermes"));
}

#[test]
fn the_public_types_are_exported() {
    fn assert_type<T>() {}
    assert_type::<LaunchTarget>();
    assert_type::<LocalOptions>();
}

#[test]
fn the_error_type_is_serializable_and_a_std_error() {
    fn assert_serialize<T: serde::Serialize>() {}
    fn assert_error<T: std::error::Error>() {}
    assert_serialize::<LaunchError>();
    assert_error::<LaunchError>();

    let error = LaunchError {
        code: "timeout".to_string(),
        message: "did not finish".to_string(),
        recovery: "try again".to_string(),
    };
    assert_eq!(error.to_string(), "did not finish");
    let json = serde_json::to_string(&error).expect("serialize");
    assert_eq!(
        json,
        r#"{"code":"timeout","message":"did not finish","recovery":"try again"}"#
    );
}

#[tokio::test]
async fn a_private_launch_link_is_extracted_without_exposing_its_ticket() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let link = link();
    let program = script(dir.path(), "hermes", &private_message(&link));
    let options = options(program);

    let target = open_local(&options).await.expect("a launch link");

    // The full link (needed to open it) carries the ticket.
    assert_eq!(target.url().as_str(), link);
    // Everything a log or UI would show is fragment-free.
    assert_eq!(target.safe_label(), "http://127.0.0.1:9120/webapp/launch");
    let rendered = format!("{target:?}");
    assert!(
        !rendered.contains(TICKET),
        "Debug leaked the ticket: {rendered}"
    );
    assert!(
        !rendered.contains("ticket="),
        "Debug leaked the fragment: {rendered}"
    );
    assert!(rendered.contains("http://127.0.0.1:9120/webapp/launch"));
}

#[tokio::test]
async fn a_sign_in_gated_https_link_is_accepted() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let program = script(
        dir.path(),
        "hermes",
        &sign_in_message("https://hermes.example/"),
    );
    let options = options(program);

    let target = open_local(&options).await.expect("a gated link");

    assert_eq!(target.url().scheme(), "https");
    assert_eq!(target.safe_label(), "https://hermes.example/");
}

#[tokio::test]
async fn a_gated_link_may_carry_only_a_profile_query() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let program = script(
        dir.path(),
        "hermes",
        &sign_in_message("https://hermes.example/?profile=work"),
    );
    let options = options(program);

    let target = open_local(&options).await.expect("a gated link");

    assert_eq!(target.url().query(), Some("profile=work"));
    assert_eq!(target.safe_label(), "https://hermes.example/");
}

#[tokio::test]
async fn a_one_use_link_message_is_understood() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let link = link();
    let program = script(dir.path(), "hermes", &one_use_message(&link));
    let options = options(program);

    let target = open_local(&options).await.expect("a one-use link");

    assert_eq!(target.url().as_str(), link);
}

#[tokio::test]
async fn only_the_launch_message_is_parsed_not_a_loose_url() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    // A banner that names some other origin before the real launch message.
    let body = format!(
        "{}\n{}",
        print_lines(&["See https://docs.example/ for help and https://other.example/ too."]),
        sign_in_message("https://hermes.example/"),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let target = open_local(&options).await.expect("the launch link");

    assert_eq!(target.url().host_str(), Some("hermes.example"));
}

#[tokio::test]
async fn a_ticket_link_is_preferred_over_a_gated_base_in_mixed_output() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let link = link();
    let body = format!(
        "{}\n{}\n{}",
        print_lines(&["Open your dashboard at https://docs.example/ first."]),
        sign_in_message("https://hermes.example/"),
        private_message(&link),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let target = open_local(&options).await.expect("the launch link");

    assert_eq!(target.url().as_str(), link);
}

#[tokio::test]
async fn open_runs_the_documented_command_with_the_profile_flag_first() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let args_file = dir.path().join("args");
    let body = format!(
        "printf '%s\\n' \"$@\" > {args}\n{message}",
        args = quote(&args_file.to_string_lossy()),
        message = private_message(&link())
    );
    let program = script(dir.path(), "hermes", &body);
    let options = LocalOptions {
        program,
        profile: Some("work".to_string()),
        ..Default::default()
    };

    open_local(&options).await.expect("a launch link");

    let recorded = lines(&args_file);
    assert_eq!(
        recorded,
        ["-p", "work", "webapp", "--no-open", "--skip-build"]
    );
}

#[tokio::test]
async fn build_runs_only_the_build_only_command() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let args_file = dir.path().join("args");
    let body = format!(
        "printf '%s\\n' \"$@\" > {args}\nprintf '%s\\n' 'Webapp bundle ready.'\n",
        args = quote(&args_file.to_string_lossy())
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    build_renderer(&options).await.expect("a build");

    assert_eq!(lines(&args_file), ["webapp", "--build-only"]);
}

#[tokio::test]
async fn explicit_home_and_renderer_reach_the_child() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("hermes-home");
    let renderer = dir.path().join("webapp-dist");
    let home_out = dir.path().join("home_out");
    let dist_out = dir.path().join("dist_out");
    let body = format!(
        "printf '%s' \"${{HERMES_HOME:-<none>}}\" > {home_out}\nprintf '%s' \"${{HERMES_WEBAPP_DIST:-<none>}}\" > {dist_out}\n{message}",
        home_out = quote(&home_out.to_string_lossy()),
        dist_out = quote(&dist_out.to_string_lossy()),
        message = private_message(&link())
    );
    let program = script(dir.path(), "hermes", &body);
    let options = LocalOptions {
        program,
        profile: None,
        home: Some(home.clone()),
        renderer: Some(renderer.clone()),
    };

    open_local(&options).await.expect("a launch link");

    assert_eq!(
        std::fs::read_to_string(&home_out).expect("home file"),
        home.to_string_lossy()
    );
    assert_eq!(
        std::fs::read_to_string(&dist_out).expect("dist file"),
        renderer.to_string_lossy()
    );
}

#[tokio::test]
async fn absent_home_and_renderer_are_never_set_by_the_launcher() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let home_out = dir.path().join("home_out");
    let dist_out = dir.path().join("dist_out");
    let body = format!(
        "printf '%s' \"${{HERMES_HOME:-<none>}}\" > {home_out}\nprintf '%s' \"${{HERMES_WEBAPP_DIST:-<none>}}\" > {dist_out}\n{message}",
        home_out = quote(&home_out.to_string_lossy()),
        dist_out = quote(&dist_out.to_string_lossy()),
        message = private_message(&link())
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    open_local(&options).await.expect("a launch link");

    // `None` means "inherit", so the child sees whatever this test process has, not a value we set.
    let expected_home = std::env::var("HERMES_HOME").unwrap_or_else(|_| "<none>".to_string());
    let expected_dist =
        std::env::var("HERMES_WEBAPP_DIST").unwrap_or_else(|_| "<none>".to_string());
    assert_eq!(
        std::fs::read_to_string(&home_out).expect("home file"),
        expected_home
    );
    assert_eq!(
        std::fs::read_to_string(&dist_out).expect("dist file"),
        expected_dist
    );
}

#[tokio::test]
async fn an_invalid_profile_is_refused_before_the_cli_starts() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let marker = dir.path().join("ran");
    let body = format!("printf '%s' ran > {}\n", quote(&marker.to_string_lossy()));
    let program = script(dir.path(), "hermes", &body);

    let invalid = [
        "Bad Name".to_string(),
        String::new(),
        "UPPER".to_string(),
        "work/../etc".to_string(),
        "-leading".to_string(),
        "x".repeat(65),
    ];
    for profile in invalid {
        let options = LocalOptions {
            program: program.clone(),
            profile: Some(profile.clone()),
            ..Default::default()
        };
        let error = open_local(&options)
            .await
            .expect_err("an invalid profile must be refused");
        assert_eq!(error.code, "invalid_profile", "profile {profile:?}");
    }
    assert!(
        !marker.exists(),
        "the CLI must not start for an invalid profile"
    );
}

/// Every way a caller could render the error: `Display`, `Debug` and the serialized form.
fn renderings(error: &LaunchError) -> Vec<String> {
    vec![
        error.to_string(),
        format!("{error:?}"),
        error.message.clone(),
        error.recovery.clone(),
        serde_json::to_string(error).expect("serialize"),
    ]
}

fn assert_never_contains(error: &LaunchError, secrets: &[&str]) {
    for rendering in renderings(error) {
        for secret in secrets {
            assert!(
                !rendering.contains(secret),
                "error leaked {secret:?} in {rendering:?}"
            );
        }
    }
}

#[tokio::test]
async fn a_failed_child_never_echoes_arbitrary_secrets() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    // Obviously-fake values, shaped like a bearer token, an API key and a password. Nothing about
    // these is real; they exist only to prove the launcher never copies raw output into an error.
    let secrets = [
        "Bearer FAKEbearer000000000000000000000000000000",
        "sk-FAKE0123456789abcdef0123456789abcdef0123456789",
        "hunter2-FAKE-not-a-real-password",
    ];
    let body = format!(
        "{to_stderr} >&2\n{to_stdout}\nexit 7\n",
        to_stderr = print_lines(&secrets),
        to_stdout = print_lines(&secrets),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("a non-zero exit");

    assert_eq!(error.code, "exit_failure");
    assert_never_contains(&error, &secrets);
    assert!(
        error.message.contains("status 7"),
        "message: {}",
        error.message
    );
}

#[tokio::test]
async fn a_failed_child_never_echoes_a_ticket_or_a_session_credential() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let ticket = "SUPERSECRETTICKETVALUE0123456789abcdefghij";
    let session = "SESSIONTOKEN0123456789012345678901234567890123";
    let body = format!(
        "{to_stderr} >&2\nexit 2\n",
        to_stderr = print_lines(&[
            &format!("boom: http://127.0.0.1:9120/webapp/launch#ticket={ticket}"),
            &format!("auth failed: hermes-session={session}"),
        ]),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("a non-zero exit");

    assert_eq!(error.code, "exit_failure");
    assert_never_contains(&error, &[ticket, session]);
    assert!(
        error.message.contains("status 2"),
        "message: {}",
        error.message
    );
}

#[tokio::test]
async fn a_successful_run_without_a_link_never_echoes_output() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let secret = "sk-FAKEunused00000000000000000000000000000000";
    let body = format!("{}\n", print_lines(&["nothing to see here", secret]));
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("no link");

    assert_eq!(error.code, "bad_output");
    assert_never_contains(&error, &[secret]);
}

#[tokio::test]
async fn a_known_webapp_off_refusal_gets_a_fixed_diagnostic() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    // The exact refusal the pinned CLI prints when the home's config keeps the Webapp off.
    let body = format!(
        "{}\nexit 1\n",
        print_lines(&[
            "The Webapp is off. Set webapp.enabled: true in /home/u/.hermes/config.yaml, then run hermes webapp again.",
        ]),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("a refusal");

    assert_eq!(error.code, "webapp_off");
    // The fixed message never carries the path or any other raw output.
    assert_never_contains(&error, &["/home/u/.hermes/config.yaml"]);
    assert!(
        error.recovery.contains("webapp.enabled"),
        "recovery: {}",
        error.recovery
    );
}

#[tokio::test]
async fn a_known_stale_gateway_refusal_gets_a_fixed_diagnostic() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let body = format!(
        "{}\nexit 1\n",
        print_lines(&[
            "The gateway started before the Webapp was turned on. Restart it to serve the Webapp: hermes gateway restart",
        ]),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("a refusal");

    assert_eq!(error.code, "gateway_stale");
    assert!(
        error.recovery.contains("gateway restart"),
        "recovery: {}",
        error.recovery
    );
}

#[tokio::test]
async fn a_known_missing_bundle_refusal_gets_a_fixed_diagnostic() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let body = format!(
        "{}\nexit 1\n",
        print_lines(&[
            "The Webapp is not built (/home/u/.hermes/webapp/dist/index.html is missing). Run hermes webapp to build it.",
        ]),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("a refusal");

    assert_eq!(error.code, "renderer_missing");
    assert_never_contains(&error, &["/home/u/.hermes/webapp/dist/index.html"]);
    assert!(
        error.recovery.contains("--build-only"),
        "recovery: {}",
        error.recovery
    );
}

#[tokio::test]
async fn a_known_host_owner_conflict_gets_a_fixed_diagnostic() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let body = format!(
        "{}\nexit 1\n",
        print_lines(&[
            "❌ A gateway already owns this host and will not serve profile 'work'.",
            "   pid 4321 from /home/u/.hermes (token super-secret-owner-token)",
        ]),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("a refusal");

    assert_eq!(error.code, "host_owner_conflict");
    assert_never_contains(&error, &["super-secret-owner-token", "/home/u/.hermes"]);
}

#[tokio::test]
async fn a_hanging_child_is_terminated_and_reaped_at_the_deadline() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let pid_file = dir.path().join("pid");
    // `exec` replaces the shell with `sleep`, so the recorded pid is the process we kill: the test
    // leaves no incidental `sleep` child behind.
    let body = format!(
        "echo $$ > {pid}\nexec sleep 30\n",
        pid = quote(&pid_file.to_string_lossy())
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let started = Instant::now();
    let error = open_local_bounded(&options, Duration::from_millis(300), 1 << 20)
        .await
        .expect_err("a hanging child");

    assert_eq!(error.code, "timeout");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the deadline was not enforced"
    );
    assert_process_gone(&pid_file);
}

#[tokio::test]
async fn an_overproducing_child_is_terminated_and_reported() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let pid_file = dir.path().join("pid");
    let body = format!(
        "echo $$ > {pid}\nwhile :; do printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\n'; done\n",
        pid = quote(&pid_file.to_string_lossy())
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let started = Instant::now();
    let error = open_local_bounded(&options, Duration::from_secs(10), 4096)
        .await
        .expect_err("an overproducing child");

    assert_eq!(error.code, "output_overflow");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the output cap was not enforced"
    );
    assert_process_gone(&pid_file);
}

#[tokio::test]
async fn a_hanging_build_is_terminated_at_the_deadline() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let program = script(dir.path(), "hermes", "exec sleep 30\n");
    let options = options(program);

    let error = build_renderer_bounded(&options, Duration::from_millis(300), 1 << 20)
        .await
        .expect_err("a hanging build");

    assert_eq!(error.code, "timeout");
}

#[tokio::test]
async fn a_child_that_exits_while_a_descendant_holds_the_pipe_still_launches() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let holder_pid = dir.path().join("holder_pid");
    let link = link();
    // The CLI exits 0, but a background descendant inherits stdout and keeps the write end of the
    // pipe open, so EOF never arrives. The launcher must still return the link, well under the
    // deadline, instead of waiting for an EOF that a detached gateway may never send.
    let body = format!(
        "sleep 30 &\nprintf '%s' \"$!\" > {holder}\n{message}\nexit 0\n",
        holder = quote(&holder_pid.to_string_lossy()),
        message = private_message(&link),
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let started = Instant::now();
    let result = open_local_bounded(&options, Duration::from_secs(5), 1 << 20).await;
    let elapsed = started.elapsed();
    // The holder must still be alive when the launcher returned, otherwise the pipe would have hit
    // EOF and the test would pass without ever exercising the post-exit drain.
    #[cfg(target_os = "linux")]
    let held = process_is_alive(&holder_pid);
    // Clean up only the pipe-holding descendant this fixture started, before asserting, so a failed
    // assertion cannot leave it behind.
    kill_fixture_child(&holder_pid);

    let target = result.expect("a launch link despite the inherited pipe");
    assert_eq!(target.url().as_str(), link);
    assert!(
        elapsed < Duration::from_secs(2),
        "the launcher waited for EOF from a detached descendant: {elapsed:?}"
    );
    #[cfg(target_os = "linux")]
    assert!(
        held,
        "the pipe holder had already exited, so the drain was not exercised"
    );
}

#[tokio::test]
async fn a_failed_build_is_reported_without_echoing_output() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let secret = "sk-FAKEbuild00000000000000000000000000000000";
    let program = script(
        dir.path(),
        "hermes",
        &format!(
            "{to_stderr} >&2\n{to_stdout}\nexit 1\n",
            to_stderr = print_lines(&[&format!("build failed: {secret}")]),
            to_stdout = print_lines(&[&format!("build failed: {secret}")]),
        ),
    );
    let options = options(program);

    let error = build_renderer(&options).await.expect_err("a failed build");

    assert_eq!(error.code, "exit_failure");
    assert_never_contains(&error, &[secret]);
}

#[tokio::test]
async fn a_link_printed_by_a_failing_child_is_not_used() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let body = format!("{}\nexit 1\n", private_message(&link()));
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("a non-zero exit");

    assert_eq!(
        error.code, "exit_failure",
        "output must only be parsed after a successful exit"
    );
}

#[tokio::test]
async fn a_link_over_a_plain_remote_transport_is_refused() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let body = private_message(&format!(
        "http://evil.example/webapp/launch#ticket={TICKET}"
    ));
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let error = open_local(&options).await.expect_err("a plain remote link");

    assert_eq!(error.code, "bad_output");
}

#[tokio::test]
async fn a_link_carrying_userinfo_is_refused() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let program = script(
        dir.path(),
        "hermes",
        &sign_in_message("https://user:pass@hermes.example/"),
    );
    let options = options(program);

    let error = open_local(&options)
        .await
        .expect_err("a link with credentials");

    assert_eq!(error.code, "bad_output");
}

#[tokio::test]
async fn the_launcher_never_retries_a_failed_command() {
    let _guard = fixtures().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let counter = dir.path().join("count");
    let body = format!(
        "n=$(cat {counter} 2>/dev/null || echo 0)\nprintf '%s' \"$((n + 1))\" > {counter}\nprintf '%s\\n' 'failed' >&2\nexit 1\n",
        counter = quote(&counter.to_string_lossy())
    );
    let program = script(dir.path(), "hermes", &body);
    let options = options(program);

    let _ = open_local(&options).await;

    assert_eq!(
        std::fs::read_to_string(&counter).expect("counter"),
        "1",
        "the command must run once"
    );
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("fixture output")
        .lines()
        .map(str::to_string)
        .collect()
}

/// SIGKILL the process that recorded `pid_file`, and only that process.
///
/// Used to clean up the pipe-holding descendant a fixture deliberately detaches. The signal is sent
/// through `/bin/sh`'s builtin `kill`, so it needs no extra crate, and only the recorded pid is
/// signalled: never a process group, never the detached gateway.
fn kill_fixture_child(pid_file: &Path) {
    let Ok(recorded) = std::fs::read_to_string(pid_file) else {
        return;
    };
    let pid = recorded.trim();
    if pid.is_empty() {
        return;
    }
    let _ = std::process::Command::new("/bin/sh")
        .args(["-c", "kill -KILL \"$1\" 2>/dev/null || true", "sh"])
        .arg(pid)
        .status();
}

/// Whether the process that recorded `pid_file` is still running (Linux: `/proc/<pid>` exists).
#[cfg(target_os = "linux")]
fn process_is_alive(pid_file: &Path) -> bool {
    let Ok(recorded) = std::fs::read_to_string(pid_file) else {
        return false;
    };
    let pid = recorded.trim();
    !pid.is_empty() && PathBuf::from(format!("/proc/{pid}")).exists()
}

/// Assert the child that recorded `pid_file` is gone, so it was terminated and reaped.
fn assert_process_gone(pid_file: &Path) {
    #[cfg(target_os = "linux")]
    {
        let pid = std::fs::read_to_string(pid_file)
            .expect("the child recorded its pid")
            .trim()
            .to_string();
        let deadline = Instant::now() + Duration::from_secs(5);
        while process_is_alive(pid_file) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !process_is_alive(pid_file),
            "child {pid} was not terminated and reaped"
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid_file;
    }
}
