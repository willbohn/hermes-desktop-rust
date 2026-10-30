//! Behavioral contract for session-scoped notification consent: grants bind a
//! top-level window label and its serialized origin, at most one prompt is open
//! per window, and asynchronous callbacks can never outlive the prompt or
//! permission they belong to. A concurrent request from another window, or an
//! identical request in the same window, must never be turned into a denial.

use hermes_desktop_rust::notifications::{NotificationConsent, Prompt};

const WINDOW: &str = "workspace-main";
const ORIGIN: &str = "https://hermes.example";

/// Requests a prompt and requires that a fresh one was opened for this window.
fn opened(consent: &mut NotificationConsent, window: &str, origin: &str) -> u64 {
    match consent.begin_prompt(window, origin) {
        Some(Prompt::Opened(serial)) => serial,
        other => panic!("expected a fresh prompt for {window}, got {other:?}"),
    }
}

/// Requests a prompt and requires that it joined the already-open identical one.
fn joined(consent: &mut NotificationConsent, window: &str, origin: &str) -> u64 {
    match consent.begin_prompt(window, origin) {
        Some(Prompt::Joined(serial)) => serial,
        other => panic!("expected to join the open prompt for {window}, got {other:?}"),
    }
}

#[test]
fn default_denies_every_window_and_origin() {
    let consent = NotificationConsent::default();
    assert!(!consent.allows(WINDOW, ORIGIN));
    assert!(!consent.allows("workspace-other", ORIGIN));
    assert!(!consent.allows(WINDOW, "https://other.example"));
    assert!(
        !consent.allows("launcher", ORIGIN),
        "the bundled launcher is never granted notification consent"
    );
}

#[test]
fn accepting_a_prompt_grants_only_the_captured_window_and_origin() {
    let mut consent = NotificationConsent::default();
    let prompt = opened(&mut consent, WINDOW, ORIGIN);
    assert!(consent.is_current(prompt, WINDOW, ORIGIN));
    assert!(
        !consent.allows(WINDOW, ORIGIN),
        "opening a prompt does not grant on its own"
    );

    assert!(consent.finish_prompt(prompt, true), "current accept grants");
    assert!(consent.allows(WINDOW, ORIGIN));
    assert!(
        !consent.allows("workspace-other", ORIGIN),
        "the grant is scoped to the requesting window"
    );
    assert!(
        !consent.allows(WINDOW, "https://elsewhere.example"),
        "the grant is scoped to the requesting origin"
    );
    assert!(
        !consent.is_current(prompt, WINDOW, ORIGIN),
        "an accepted prompt is consumed"
    );
}

#[test]
fn a_prompt_in_one_window_never_denies_a_request_in_another() {
    // Regression: a second request while a prompt was open used to be refused a
    // prompt (returning None), which the native layer turned into a permanent
    // engine denial. Each window now owns its own prompt.
    let mut consent = NotificationConsent::default();
    let a = opened(&mut consent, "window-a", "https://a.example");
    let b = opened(&mut consent, "window-b", "https://b.example");
    assert_ne!(a, b);
    assert!(consent.is_current(a, "window-a", "https://a.example"));
    assert!(consent.is_current(b, "window-b", "https://b.example"));

    assert!(consent.finish_prompt(b, true));
    assert!(consent.allows("window-b", "https://b.example"));
    assert!(
        !consent.allows("window-a", "https://a.example"),
        "finishing one window's prompt never grants another"
    );
    assert!(
        consent.is_current(a, "window-a", "https://a.example"),
        "the other window's prompt survives"
    );
}

#[test]
fn an_identical_request_joins_the_open_prompt_instead_of_denying() {
    let mut consent = NotificationConsent::default();
    let first = opened(&mut consent, WINDOW, ORIGIN);
    let second = joined(&mut consent, WINDOW, ORIGIN);
    assert_eq!(
        first, second,
        "the same window and origin share a single decision"
    );
    assert!(consent.is_current(first, WINDOW, ORIGIN));
    assert!(consent.finish_prompt(first, true));
    assert!(consent.allows(WINDOW, ORIGIN));
}

#[test]
fn a_new_origin_in_the_same_window_replaces_a_stale_prompt() {
    let mut consent = NotificationConsent::default();
    let stale = opened(&mut consent, WINDOW, "https://old.example");
    let fresh = opened(&mut consent, WINDOW, "https://new.example");
    assert_ne!(stale, fresh);
    assert!(
        !consent.is_current(stale, WINDOW, "https://old.example"),
        "the stale origin's prompt is dropped"
    );
    assert!(consent.is_current(fresh, WINDOW, "https://new.example"));
    assert!(
        !consent.finish_prompt(stale, true),
        "a stale callback cannot grant"
    );
    assert!(!consent.allows(WINDOW, "https://old.example"));
    assert!(consent.finish_prompt(fresh, true));
    assert!(consent.allows(WINDOW, "https://new.example"));
}

#[test]
fn rejecting_a_prompt_does_not_grant() {
    let mut consent = NotificationConsent::default();
    let prompt = opened(&mut consent, WINDOW, ORIGIN);
    assert!(
        !consent.finish_prompt(prompt, false),
        "a rejection is not a grant"
    );
    assert!(!consent.allows(WINDOW, ORIGIN));
    assert!(!consent.is_current(prompt, WINDOW, ORIGIN));
}

#[test]
fn a_rejected_replacement_preserves_existing_permission() {
    let mut consent = NotificationConsent::default();
    let first = opened(&mut consent, WINDOW, ORIGIN);
    assert!(consent.finish_prompt(first, true));

    let second = opened(&mut consent, WINDOW, ORIGIN);
    assert_ne!(first, second, "each prompt receives a fresh serial");
    assert!(
        !consent.finish_prompt(second, false),
        "a rejection is not a grant"
    );
    assert!(
        consent.allows(WINDOW, ORIGIN),
        "rejecting a replacement keeps the earlier permission"
    );
}

#[test]
fn revoke_clears_grants_and_invalidates_pending_callbacks() {
    let mut consent = NotificationConsent::default();
    let granted = opened(&mut consent, WINDOW, ORIGIN);
    assert!(consent.finish_prompt(granted, true));
    let stalled = opened(&mut consent, WINDOW, ORIGIN);

    consent.revoke();
    assert!(!consent.allows(WINDOW, ORIGIN), "revoke clears grants");
    assert!(
        !consent.is_current(stalled, WINDOW, ORIGIN),
        "revoke drops the pending prompt"
    );
    assert!(
        !consent.finish_prompt(stalled, true),
        "a stale callback cannot grant after revoke"
    );
    assert!(!consent.allows(WINDOW, ORIGIN));

    let fresh = opened(&mut consent, WINDOW, ORIGIN);
    assert_ne!(stalled, fresh);
    assert!(
        !consent.finish_prompt(stalled, false),
        "a stale callback cannot clear a newer prompt"
    );
    assert!(
        consent.is_current(fresh, WINDOW, ORIGIN),
        "the newer prompt survives the stale callback"
    );
    assert!(consent.finish_prompt(fresh, true));
    assert!(consent.allows(WINDOW, ORIGIN));
}

#[test]
fn closing_a_window_forgets_only_its_grant_and_prompt() {
    let mut consent = NotificationConsent::default();
    let a = opened(&mut consent, "window-a", "https://a.example");
    assert!(consent.finish_prompt(a, true));
    let b = opened(&mut consent, "window-b", "https://b.example");
    assert!(consent.finish_prompt(b, true));

    // Both windows ask again, so each holds its own pending prompt at once.
    let pending_a = opened(&mut consent, "window-a", "https://a.example");
    let pending_b = opened(&mut consent, "window-b", "https://b.example");

    consent.forget("window-a");
    assert!(
        !consent.allows("window-a", "https://a.example"),
        "forget removes the closed window's grant"
    );
    assert!(
        consent.allows("window-b", "https://b.example"),
        "forget leaves other windows untouched"
    );
    assert!(
        !consent.is_current(pending_a, "window-a", "https://a.example"),
        "forget drops the closed window's pending prompt"
    );
    assert!(
        consent.is_current(pending_b, "window-b", "https://b.example"),
        "forget leaves an unrelated pending prompt intact"
    );
    assert!(
        !consent.finish_prompt(pending_a, true),
        "the closed window's stale callback cannot grant"
    );

    let reopened = opened(&mut consent, "window-a", "https://a.example");
    assert_ne!(pending_a, reopened);
}

#[test]
fn prompt_serials_are_unique_and_never_reused() {
    let mut consent = NotificationConsent::default();
    let mut seen = Vec::new();
    for _ in 0..8 {
        let prompt = opened(&mut consent, WINDOW, ORIGIN);
        assert!(!seen.contains(&prompt), "serial {prompt} was reused");
        seen.push(prompt);
        assert!(!consent.finish_prompt(prompt, false));
    }
}

#[test]
fn re_enabling_never_revives_a_queued_delivery_from_an_old_grant() {
    let mut consent = NotificationConsent::default();
    assert_eq!(consent.grant_id(WINDOW, ORIGIN), None);
    let first = opened(&mut consent, WINDOW, ORIGIN);
    assert!(consent.finish_prompt(first, true));
    let queued = consent.grant_id(WINDOW, ORIGIN).unwrap();
    consent.revoke();
    assert_eq!(consent.grant_id(WINDOW, ORIGIN), None);
    let second = opened(&mut consent, WINDOW, ORIGIN);
    assert!(consent.finish_prompt(second, true));
    assert_ne!(consent.grant_id(WINDOW, ORIGIN), Some(queued));
}

#[test]
fn a_workspace_grant_never_leaks_to_another_window() {
    let mut consent = NotificationConsent::default();
    let prompt = opened(&mut consent, "workspace-1", ORIGIN);
    assert!(consent.finish_prompt(prompt, true));
    assert!(consent.allows("workspace-1", ORIGIN));
    assert!(
        !consent.allows("workspace-2", ORIGIN),
        "a grant is bound to the window that requested it"
    );
}
