#![allow(dead_code)]

use std::sync::Mutex;

use crate::quota::profile::Meter;
use crate::support::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    RoundRobin,
    Sticky,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub index: usize,
    pub slot: usize,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "account {} (slot {})", self.index + 1, self.slot + 1)
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub round_robin: bool,
    pub sticky: u64,
}

impl Settings {
    pub fn mode(&self) -> Mode {
        if self.round_robin {
            Mode::RoundRobin
        } else {
            Mode::Sticky
        }
    }
}

struct State {
    cursor: usize,
    used: u64,
    current: usize,
}

pub struct Guard {
    settings: Settings,
    accounts: usize,
    exhausted: Mutex<Vec<bool>>,
    cooldown: Mutex<Vec<i64>>,
    state: Mutex<State>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_secs() as i64)
        .unwrap_or(0)
}

impl Guard {
    pub fn new(settings: Settings, accounts: usize) -> AppResult<Guard> {
        if accounts == 0 {
            return Err(AppError::quota(
                "no account is available to serve a request",
            ));
        }
        Ok(Guard {
            settings,
            accounts,
            exhausted: Mutex::new(vec![false; accounts]),
            cooldown: Mutex::new(vec![0; accounts]),
            state: Mutex::new(State {
                cursor: 0,
                used: 0,
                current: 0,
            }),
        })
    }

    pub fn accounts(&self) -> usize {
        self.accounts
    }

    pub fn mode(&self) -> Mode {
        self.settings.mode()
    }

    pub fn has_room(&self) -> bool {
        match self.exhausted.lock() {
            Ok(guard) => guard.iter().any(|done| !done),
            Err(_) => false,
        }
    }

    pub fn acquire(&self) -> AppResult<Choice> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AppError::quota("the account cursor was poisoned"))?;
        let spent = self
            .exhausted
            .lock()
            .map_err(|_| AppError::quota("the account ledger was poisoned"))?;

        if spent.iter().all(|done| *done) {
            return Err(AppError::quota(
                "every account has used up its plan limit, stopping",
            ));
        }

        if self.settings.mode() == Mode::Sticky {
            if let Some((index, _)) = spent
                .iter()
                .enumerate()
                .find(|(index, done)| !**done && self.ready(*index))
            {
                state.cursor = index;
                state.used = 1;
                state.current = index;
                return Ok(Choice { index, slot: index });
            }
        }

        if state.used < self.settings.sticky
            && state.current < self.accounts
            && !spent[state.current]
            && self.ready(state.current)
        {
            state.used += 1;
            return Ok(Choice {
                index: state.current,
                slot: state.current,
            });
        }

        let start = if state.used > 0 {
            (state.current + 1) % self.accounts
        } else {
            state.cursor % self.accounts
        };
        let mut cooling: Option<usize> = None;
        for step in 0..self.accounts {
            let index = (start + step) % self.accounts;
            if spent[index] {
                continue;
            }
            if !self.ready(index) {
                cooling.get_or_insert(index);
                continue;
            }
            state.cursor = (index + 1) % self.accounts;
            state.used = 1;
            state.current = index;
            return Ok(Choice { index, slot: index });
        }

        if let Some(index) = cooling {
            state.cursor = (index + 1) % self.accounts;
            state.used = 1;
            state.current = index;
            return Ok(Choice { index, slot: index });
        }

        Err(AppError::quota(
            "every account has used up its plan limit, stopping",
        ))
    }

    pub fn mark_exhausted(&self, slot: usize, meter: &Meter) {
        if slot >= self.accounts {
            return;
        }
        if let Ok(mut spent) = self.exhausted.lock() {
            if meter.is_empty() {
                spent[slot] = true;
            }
        }
    }

    pub fn blip(&self, slot: usize, seconds: u64) {
        if slot >= self.accounts {
            return;
        }
        let until = now_unix() + seconds as i64;
        if let Ok(mut cooldown) = self.cooldown.lock() {
            cooldown[slot] = until;
        }
    }

    pub fn ready(&self, slot: usize) -> bool {
        if slot >= self.accounts {
            return false;
        }
        match self.cooldown.lock() {
            Ok(cooldown) => cooldown[slot] <= now_unix(),
            Err(_) => true,
        }
    }

    pub fn mark_live(&self, slot: usize) {
        if slot >= self.accounts {
            return;
        }
        if let Ok(mut spent) = self.exhausted.lock() {
            spent[slot] = false;
        }
    }

    pub fn is_spent(&self, slot: usize) -> bool {
        if slot >= self.accounts {
            return true;
        }
        self.exhausted
            .lock()
            .map(|spent| spent[slot])
            .unwrap_or(true)
    }

    pub fn retire(&self, slot: usize) {
        if slot >= self.accounts {
            return;
        }
        if let Ok(mut spent) = self.exhausted.lock() {
            spent[slot] = true;
        }
        if let Ok(mut state) = self.state.lock() {
            if state.current == slot {
                state.used = self.settings.sticky;
            }
        }
    }

    pub fn live(&self) -> usize {
        match self.exhausted.lock() {
            Ok(spent) => spent.iter().filter(|done| !**done).count(),
            Err(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::profile::HERMES;

    fn meter(remaining: u64) -> Meter {
        Meter {
            name: HERMES.into(),
            current: 0,
            remaining,
        }
    }

    #[test]
    fn sticky_keeps_one_account_for_the_whole_window() {
        let guard = Guard::new(
            Settings {
                round_robin: true,
                sticky: 3,
            },
            4,
        )
        .unwrap();
        let picks: Vec<usize> = (0..3).map(|_| guard.acquire().unwrap().index).collect();
        assert_eq!(picks, vec![0, 0, 0]);
        let next = guard.acquire().unwrap();
        assert_eq!(next.index, 1);
        assert_eq!(guard.mode(), Mode::RoundRobin);
    }

    #[test]
    fn the_cursor_rounds_over_every_account() {
        let guard = Guard::new(
            Settings {
                round_robin: true,
                sticky: 1,
            },
            3,
        )
        .unwrap();
        let picks: Vec<usize> = (0..6).map(|_| guard.acquire().unwrap().index).collect();
        assert_eq!(picks, vec![0, 1, 2, 0, 1, 2]);
    }

    #[test]
    fn an_exhausted_account_is_skipped() {
        let guard = Guard::new(
            Settings {
                round_robin: true,
                sticky: 1,
            },
            3,
        )
        .unwrap();
        guard.mark_exhausted(0, &meter(0));
        assert_eq!(guard.acquire().unwrap().index, 1);
        assert_eq!(guard.acquire().unwrap().index, 2);
        assert_eq!(guard.acquire().unwrap().index, 1);
        assert_eq!(guard.live(), 2);
    }

    #[test]
    fn every_account_empty_stops_the_bridge() {
        let guard = Guard::new(
            Settings {
                round_robin: true,
                sticky: 1,
            },
            2,
        )
        .unwrap();
        guard.retire(0);
        guard.retire(1);
        assert!(!guard.has_room());
        assert!(guard.acquire().is_err());
    }

    #[test]
    fn a_live_meter_never_retires_an_account() {
        let guard = Guard::new(
            Settings {
                round_robin: true,
                sticky: 1,
            },
            2,
        )
        .unwrap();
        guard.mark_exhausted(0, &meter(5));
        assert_eq!(guard.live(), 2);
    }

    #[test]
    fn no_account_is_refused_up_front() {
        assert!(Guard::new(
            Settings {
                round_robin: true,
                sticky: 1,
            },
            0,
        )
        .is_err());
    }

    #[test]
    fn the_sticky_window_restarts_after_a_retire() {
        let guard = Guard::new(
            Settings {
                round_robin: false,
                sticky: 3,
            },
            2,
        )
        .unwrap();
        assert_eq!(guard.acquire().unwrap().index, 0);
        guard.retire(0);
        assert_eq!(guard.acquire().unwrap().index, 1);
    }
}
