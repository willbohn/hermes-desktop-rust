//! Session-scoped consent; dialog IDs bind asynchronous results to current user intent.
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct DownloadConsent {
    directory: Option<PathBuf>,
    prompt: Option<u64>,
    serial: u64,
}

impl DownloadConsent {
    pub fn directory(&self) -> Option<&Path> {
        self.directory.as_deref()
    }

    pub fn begin_prompt(&mut self) -> Option<u64> {
        if self.prompt.is_some() {
            return None;
        }
        self.serial = self.serial.checked_add(1)?;
        self.prompt = Some(self.serial);
        self.prompt
    }

    pub fn is_current(&self, prompt: u64) -> bool {
        self.prompt == Some(prompt)
    }

    pub fn finish_prompt(&mut self, prompt: u64, directory: Option<PathBuf>) {
        if !self.is_current(prompt) {
            return;
        }
        self.prompt = None;
        if let Some(directory) = directory {
            self.directory = Some(directory);
        }
    }

    pub fn revoke(&mut self) {
        self.directory = None;
        // Recovery also invalidates pending callbacks; an old dialog cannot re-grant access.
        self.prompt = None;
    }
}
