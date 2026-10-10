//! The live CLI processes: at most one per session, started on demand or
//! ahead of time (prewarm), and stopped when idle so a long day of sessions
//! does not keep dozens of processes alive.
//!
//! Reaping (`reap`): a Ready session idle for longer than `Limits::idle` is
//! stopped, and beyond `Limits::max_live` the least recently used idle ones
//! go first. A session that is running a turn or waiting on a prompt is never
//! reaped. A reaped session's transcript is on disk; its next message resumes
//! it (`--resume`), so to the person it only "sleeps".

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::cli::Session;
use crate::log;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub idle: Duration,
    pub max_live: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { idle: Duration::from_secs(10 * 60), max_live: 6 }
    }
}

/// One live session as reaping sees it.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub idle_for: Duration,
    /// Running a turn, or waiting on a prompt: never reaped.
    pub active: bool,
}

/// Which sessions to stop: every idle one past the limit, then the least
/// recently used idle ones until at most `max_live` remain.
pub fn choose(cands: &[Candidate], limits: Limits) -> Vec<String> {
    let mut out: Vec<String> = cands.iter().filter(|c| !c.active && c.idle_for > limits.idle).map(|c| c.id.clone()).collect();
    let mut left = cands.len() - out.len();
    let mut idle: Vec<&Candidate> = cands.iter().filter(|c| !c.active && !out.contains(&c.id)).collect();
    idle.sort_by(|a, b| b.idle_for.cmp(&a.idle_for));
    for c in idle {
        if left <= limits.max_live {
            break;
        }
        out.push(c.id.clone());
        left -= 1;
    }
    out
}

#[derive(Default)]
pub struct Manager {
    live: Mutex<HashMap<String, Arc<Session>>>,
    pub limits: Limits,
}

impl Manager {
    pub fn new(limits: Limits) -> Manager {
        Manager { live: Mutex::default(), limits }
    }

    /// Track a started session; one already live under that id is stopped.
    pub fn insert(&self, s: Arc<Session>) {
        let old = self.live.lock().unwrap().insert(s.id.clone(), s);
        if let Some(old) = old {
            old.stop();
        }
        // Room for the new one within the cap.
        self.reap();
    }

    /// The session, if its process is alive.
    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.live.lock().unwrap().get(id).filter(|s| s.is_running()).cloned()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.live.lock().unwrap().remove(id)
    }

    pub fn running_ids(&self) -> Vec<String> {
        self.live.lock().unwrap().iter().filter(|(_, s)| s.is_running()).map(|(id, _)| id.clone()).collect()
    }

    pub fn stop_all(&self) {
        let all: Vec<_> = self.live.lock().unwrap().drain().map(|(_, s)| s).collect();
        for s in all {
            s.stop();
        }
    }

    /// Stop what `choose` picks (and forget processes that already ended).
    pub fn reap(&self) -> Vec<String> {
        let picked = {
            let mut live = self.live.lock().unwrap();
            live.retain(|_, s| s.is_running());
            let cands: Vec<Candidate> = live.values().map(|s| Candidate { id: s.id.clone(), idle_for: s.idle_for(), active: s.is_busy() || s.needs_you() }).collect();
            let picked = choose(&cands, self.limits);
            picked.iter().filter_map(|id| live.remove(id)).collect::<Vec<_>>()
        };
        for s in &picked {
            log::write("info", &format!("session {} is idle; stopping its process", s.id));
            s.stop();
        }
        picked.iter().map(|s| s.id.clone()).collect()
    }

    /// Reap every 30 s for as long as the manager lives.
    pub fn spawn_reaper(self: &Arc<Manager>) {
        let weak = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("reaper".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                match weak.upgrade() {
                    Some(m) => {
                        m.reap();
                    }
                    None => return,
                }
            })
            .expect("the reaper thread starts");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: &str, idle_secs: u64, active: bool) -> Candidate {
        Candidate { id: id.into(), idle_for: Duration::from_secs(idle_secs), active }
    }

    #[test]
    fn idle_ones_go_and_busy_ones_stay() {
        let l = Limits { idle: Duration::from_secs(600), max_live: 6 };
        assert_eq!(choose(&[c("a", 601, false), c("b", 599, false), c("c", 5000, true)], l), ["a"]);
    }

    #[test]
    fn over_the_cap_the_least_recently_used_go_first() {
        let l = Limits { idle: Duration::from_secs(600), max_live: 2 };
        let got = choose(&[c("a", 10, false), c("b", 300, false), c("c", 20, true), c("d", 200, false)], l);
        assert_eq!(got, ["b", "d"]);
        // Active sessions are kept even when that leaves more than the cap.
        let busy = choose(&[c("a", 1, true), c("b", 1, true), c("c", 1, true)], l);
        assert!(busy.is_empty());
    }
}
