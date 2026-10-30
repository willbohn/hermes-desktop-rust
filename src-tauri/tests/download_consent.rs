use hermes_desktop_rust::downloads::DownloadConsent;
use std::path::Path;

#[test]
fn revoking_recovers_a_missing_callback_without_accepting_stale_consent() {
    let mut consent = DownloadConsent::default();
    assert!(consent.directory().is_none());
    let stalled = consent.begin_prompt().unwrap();
    assert!(
        consent.begin_prompt().is_none(),
        "one native prompt at a time"
    );

    // A dialog never returned. The native revoke action must permit recovery.
    consent.revoke();
    let current = consent
        .begin_prompt()
        .expect("revocation resets a stalled dialog");
    assert_ne!(stalled, current);
    consent.finish_prompt(stalled, Some("stale-folder".into()));
    assert!(
        consent.directory().is_none(),
        "an old picker must not restore permission"
    );
    assert!(
        consent.is_current(current),
        "old callbacks cannot unlock a newer prompt"
    );
    consent.finish_prompt(current, Some("chosen-folder".into()));
    assert_eq!(consent.directory(), Some(Path::new("chosen-folder")));

    let replacement = consent.begin_prompt().unwrap();
    consent.finish_prompt(replacement, None);
    assert_eq!(
        consent.directory(),
        Some(Path::new("chosen-folder")),
        "cancelling a replacement preserves consent"
    );
    consent.revoke();
    consent.finish_prompt(current, Some("chosen-folder".into()));
    assert!(
        consent.directory().is_none(),
        "completed callbacks cannot undo a later revoke"
    );
}
