//! Session-scoped notification consent. A prompt token binds an asynchronous
//! native decision to the exact window label and serialized origin that asked
//! for it, so a stale callback can never grant or clear a newer prompt.
//!
//! At most one prompt is open per window. A second window is never refused a
//! prompt because another is open, and an identical request (same window and
//! origin) joins the open prompt instead of producing a fresh engine denial.
//!
//! Only window labels and serialized origins are stored; complete URLs are not.

#[derive(Default)]
pub struct NotificationConsent {
    grants: Vec<Grant>,
    prompts: Vec<PendingPrompt>,
    serial: u64,
}

/// The outcome of asking to open the single prompt for a window and origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// A fresh prompt is now open; the caller shows one dialog and resolves
    /// `serial`.
    Opened(u64),
    /// An identical prompt is already open for this window and origin; the
    /// caller joins it rather than denying the request.
    Joined(u64),
}

impl Prompt {
    /// The serial of the opened or joined prompt.
    pub fn serial(self) -> u64 {
        match self {
            Prompt::Opened(serial) | Prompt::Joined(serial) => serial,
        }
    }
}

/// A granted pair: the top-level window label and the serialized origin it may
/// post notifications from. Kept as owned strings; no URL is retained.
struct Grant {
    window: String,
    origin: String,
    serial: u64,
}

/// A prompt awaiting a native decision, capturing the window and origin that
/// requested it.
struct PendingPrompt {
    serial: u64,
    window: String,
    origin: String,
}

impl NotificationConsent {
    /// Whether `window` from `origin` may currently deliver notifications.
    pub fn allows(&self, window: &str, origin: &str) -> bool {
        self.grant_id(window, origin).is_some()
    }

    /// Snapshot a grant for asynchronous delivery; re-enabling cannot revive old queued work.
    pub fn grant_id(&self, window: &str, origin: &str) -> Option<u64> {
        self.grants
            .iter()
            .find(|grant| grant.window == window && grant.origin == origin)
            .map(|grant| grant.serial)
    }

    /// Opens, or joins, the single prompt for `window`. A prompt already open
    /// for the same window and origin is returned as [`Prompt::Joined`]; a
    /// prompt open in another window never blocks this one. Returns `None` only
    /// when the serial would overflow.
    pub fn begin_prompt(&mut self, window: &str, origin: &str) -> Option<Prompt> {
        if let Some(existing) = self.prompts.iter().find(|pending| pending.window == window) {
            if existing.origin == origin {
                return Some(Prompt::Joined(existing.serial));
            }
            // A prompt captured for a different origin is stale for this request
            // (navigation normally clears it). Drop it and open a fresh prompt
            // rather than deny a legitimate request.
            self.prompts.retain(|pending| pending.window != window);
        }
        self.serial = self.serial.checked_add(1)?;
        self.prompts.push(PendingPrompt {
            serial: self.serial,
            window: window.to_owned(),
            origin: origin.to_owned(),
        });
        Some(Prompt::Opened(self.serial))
    }

    /// Whether `prompt` is the open prompt for exactly this `window`/`origin`.
    pub fn is_current(&self, prompt: u64, window: &str, origin: &str) -> bool {
        self.prompts.iter().any(|pending| {
            pending.serial == prompt && pending.window == window && pending.origin == origin
        })
    }

    /// Resolves the open prompt. Returns `true` only when a current prompt is
    /// accepted, granting its captured window and origin. A stale callback is
    /// ignored and cannot grant or clear a newer prompt; a rejected replacement
    /// preserves any existing grant.
    pub fn finish_prompt(&mut self, prompt: u64, accepted: bool) -> bool {
        let Some(index) = self
            .prompts
            .iter()
            .position(|pending| pending.serial == prompt)
        else {
            // A stale callback must leave a newer prompt untouched.
            return false;
        };
        let pending = self.prompts.remove(index);
        if !accepted {
            // Rejection clears only the prompt, keeping prior permission.
            return false;
        }
        let already_granted = self
            .grants
            .iter()
            .any(|grant| grant.window == pending.window && grant.origin == pending.origin);
        if !already_granted {
            self.grants.push(Grant {
                window: pending.window,
                origin: pending.origin,
                serial: pending.serial,
            });
        }
        true
    }

    /// Clears every grant and invalidates any pending prompt, so an old decision
    /// cannot re-grant access.
    pub fn revoke(&mut self) {
        self.grants.clear();
        self.prompts.clear();
    }

    /// Drops every grant for `window` and invalidates only its pending prompt.
    pub fn forget(&mut self, window: &str) {
        self.grants.retain(|grant| grant.window != window);
        self.prompts.retain(|pending| pending.window != window);
    }
}
