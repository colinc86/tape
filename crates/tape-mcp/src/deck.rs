//! Deck state — handle map of loaded/recording tapes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tape_format::reader::RawTape;
use tape_format::tracks::Track;

#[derive(Debug, Clone)]
pub struct Loaded {
    pub path: std::path::PathBuf,
    pub meta_yaml: String,
    pub liner_md: String,
    pub tracks: Vec<Track>,
    pub raw: Arc<RawTape>,
    /// True if this handle is currently being recorded into (from `tape.record`).
    pub recording: bool,
}

#[derive(Debug, Default)]
pub struct DeckState {
    handles: HashMap<String, Loaded>,
    next: u64,
}

impl DeckState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a new opaque handle.
    pub fn mint_handle(&mut self) -> String {
        self.next += 1;
        format!("tape:{:08x}", self.next)
    }

    pub fn put(&mut self, handle: String, loaded: Loaded) {
        self.handles.insert(handle, loaded);
    }

    pub fn get(&self, handle: &str) -> Option<&Loaded> {
        self.handles.get(handle)
    }

    pub fn get_mut(&mut self, handle: &str) -> Option<&mut Loaded> {
        self.handles.get_mut(handle)
    }

    pub fn remove(&mut self, handle: &str) -> Option<Loaded> {
        self.handles.remove(handle)
    }
}

/// Shareable deck handle.
#[derive(Debug, Clone, Default)]
pub struct Deck {
    pub state: Arc<Mutex<DeckState>>,
}

impl Deck {
    pub fn new() -> Self {
        Self::default()
    }
}
