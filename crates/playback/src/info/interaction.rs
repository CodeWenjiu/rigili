#[derive(Default)]
pub(super) struct ToggleState {
    pub(super) active: bool,
    pub(super) count: u64,
    pub(super) busy: bool,
    touched: bool,
}

impl ToggleState {
    pub(super) fn with_count(count: u64) -> Self {
        Self {
            count,
            ..Self::default()
        }
    }

    pub(super) fn accept(&mut self, active: bool) {
        if !self.touched {
            self.active = active;
        }
    }

    pub(super) fn accept_count(&mut self, count: u64) {
        if !self.touched {
            self.count = count;
        }
    }

    pub(super) fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        self.touched |= busy;
    }

    pub(super) fn apply(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.count = if active {
                self.count.saturating_add(1)
            } else {
                self.count.saturating_sub(1)
            };
        }
        self.touched = true;
        self.busy = false;
    }
}

#[derive(Default)]
pub(super) struct CoinState {
    pub(super) given: u32,
    pub(super) count: u64,
    pub(super) busy: bool,
    touched: bool,
}

impl CoinState {
    pub(super) fn with_count(count: u64) -> Self {
        Self {
            count,
            ..Self::default()
        }
    }

    pub(super) fn accept(&mut self, given: u32) {
        if !self.touched {
            self.given = given.min(2);
        }
    }

    pub(super) fn accept_count(&mut self, count: u64) {
        if !self.touched {
            self.count = count;
        }
    }

    pub(super) fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        self.touched |= busy;
    }

    pub(super) fn grant(&mut self) {
        if self.given < 2 {
            self.given += 1;
            self.count = self.count.saturating_add(1);
        }
        self.touched = true;
        self.busy = false;
    }
}

#[derive(Default)]
pub(super) struct FlagState {
    pub(super) active: bool,
    pub(super) busy: bool,
    touched: bool,
}

impl FlagState {
    pub(super) fn accept(&mut self, active: bool) {
        if !self.touched {
            self.active = active;
        }
    }

    pub(super) fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        self.touched |= busy;
    }

    pub(super) fn apply(&mut self, active: bool) {
        self.active = active;
        self.touched = true;
        self.busy = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{CoinState, FlagState, ToggleState};

    #[test]
    fn toggle_tracks_count_and_ignores_stale_server_state() {
        let mut toggle = ToggleState::with_count(10);
        toggle.accept(true);
        assert!(toggle.active);
        assert_eq!(toggle.count, 10);

        toggle.accept_count(20);
        assert_eq!(toggle.count, 20);

        toggle.set_busy(true);
        toggle.apply(false);
        assert!(!toggle.busy);
        assert_eq!(toggle.count, 19);

        toggle.apply(true);
        assert_eq!(toggle.count, 20);

        toggle.accept(false);
        assert!(toggle.active);
        toggle.accept_count(99);
        assert_eq!(toggle.count, 20);
    }

    #[test]
    fn coin_grants_at_most_two_coins() {
        let mut coin = CoinState::with_count(5);
        coin.accept(1);
        assert_eq!(coin.given, 1);

        coin.grant();
        assert_eq!((coin.given, coin.count), (2, 6));
        coin.grant();
        assert_eq!((coin.given, coin.count), (2, 6));

        coin.set_busy(true);
        coin.grant();
        assert!(!coin.busy);
        coin.accept(0);
        assert_eq!(coin.given, 2);
    }

    #[test]
    fn flag_accepts_server_state_until_touched() {
        let mut flag = FlagState::default();
        flag.accept(true);
        assert!(flag.active);

        flag.set_busy(true);
        flag.accept(false);
        assert!(flag.active);

        flag.apply(false);
        assert!(!flag.active);
        assert!(!flag.busy);
    }
}
