//! Delta events streamed from the engine to shells (PLAN.md §4.1).
//!
//! Shells subscribe once and update incrementally; they never poll. Over
//! UniFFI these become typed callbacks; over the C ABI, JSON messages.

/// Everything a shell can observe from the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    ScanStarted {
        library_id: i64,
    },
    ScanProgress {
        library_id: i64,
        done: u64,
        total: u64,
    },
    ScanFinished {
        library_id: i64,
        added: u64,
        removed: u64,
        unchanged: u64,
    },
    /// The game list for a system changed; shells re-query just that system.
    GamesChanged {
        system_id: i64,
    },
    LaunchStarted {
        game_id: i64,
        session_id: i64,
    },
    LaunchEnded {
        game_id: i64,
        session_id: i64,
        duration_s: i64,
    },
    /// Non-fatal problem worth surfacing (unreadable dir, bad gamelist entry).
    Warning {
        code: String,
        context: String,
    },
}

/// Callback used by shells; must be cheap and non-blocking.
pub type EventSink<'a> = dyn FnMut(Event) + 'a;
