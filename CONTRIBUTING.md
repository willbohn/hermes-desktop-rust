# Contributing

This is an independent Rust desktop-host experiment, not an official Nous Research release. Start by discussing substantial changes in this repository. Keep changes focused and preserve the upstream agent, session and plugin contracts.

## AI assistance

AI-assisted work is welcome. Disclose material assistance in the pull request, explain your change, and report checks actually run. The submitting maintainer remains responsible for reviewing and understanding the code. Do not claim human review that did not happen. AI review does not replace required human or legal approval.

## Review

Use a feature branch and a pull request; never commit credentials, private conversations or local profiles. Tests should exercise behavior, not assert source text. Security-sensitive changes require focused independent review. CI establishes only the checks it runs, not platform parity or human understanding. See SECURITY.md for private reporting.

Preserve upstream copyright notices. Changes intended for NousResearch/hermes-agent must follow its current contribution and AI policies independently; this repository does not authorize upstream submission or imply upstream acceptance. Coordinate a native-shell migration with maintainers before proposing replacement.

Add user-visible changes to CHANGELOG.md. Tested build and test commands will be kept in README.md and AGENTS.md as they are established.
