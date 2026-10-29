//! Workspace URL policy is enforced before the webview or network sees a target.
use hermes_desktop_rust::policy::remote_workspace_url;

#[test]
fn accepts_secure_workspace_but_rejects_credentials_and_unsafe_transports() {
    for target in [
        "https://hermes.example/workspace/",
        "http://127.0.0.1:9120/",
        "http://[::1]:9120/",
    ] {
        assert!(
            remote_workspace_url(target).is_ok(),
            "expected a workspace: {target}"
        );
    }
    for target in [
        "http://hermes.example/",
        "http://localhost.evil.example/",
        "https://user:password@hermes.example/",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "data:text/html,hello",
        "https://hermes.example/?token=secret",
        "https://hermes.example/#token=secret",
        "https://hermes.example/\n",
    ] {
        assert!(
            remote_workspace_url(target).is_err(),
            "must refuse: {target}"
        );
    }
}
