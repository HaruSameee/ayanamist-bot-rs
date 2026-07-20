use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const TIME_LIMIT: Duration = Duration::from_secs(120);
pub const MAX_ATTEMPTS: u32 = 3;
pub const FAILURE_WINDOW: Duration = Duration::from_secs(5 * 60);
pub const FAILURE_LIMIT: usize = 6;
pub const COOLDOWN: Duration = Duration::from_secs(10 * 60);

pub enum SubmitOutcome {
    Correct,
    Wrong { invalidated: bool },
    Expired,
}

pub struct Challenge {
    pub answer: String,
    pub expires_at: Instant,
    pub attempts: u32,
}

impl Challenge {
    pub fn new(answer: String, now: Instant) -> Self {
        Self {
            answer,
            expires_at: now + TIME_LIMIT,
            attempts: 0,
        }
    }

    pub fn is_expired(&self, now: Instant) -> bool {
        now > self.expires_at
    }

    pub fn submit(&mut self, input: &str, now: Instant) -> SubmitOutcome {
        if self.is_expired(now) {
            return SubmitOutcome::Expired;
        }
        if input.trim().eq_ignore_ascii_case(&self.answer) {
            return SubmitOutcome::Correct;
        }
        self.attempts += 1;
        SubmitOutcome::Wrong {
            invalidated: self.attempts >= MAX_ATTEMPTS,
        }
    }
}

#[derive(Default)]
pub struct FailureTracker {
    failures: VecDeque<Instant>,
    cooldown_until: Option<Instant>,
}

impl FailureTracker {
    pub fn record_failure(&mut self, now: Instant) {
        self.prune(now);
        self.failures.push_back(now);
        if self.failures.len() >= FAILURE_LIMIT {
            self.cooldown_until = Some(now + COOLDOWN);
        }
    }

    pub fn is_in_cooldown(&self, now: Instant) -> bool {
        self.cooldown_until.is_some_and(|until| now < until)
    }

    /// 失敗記録もクールダウンも残っていないアイドル状態かどうか
    pub fn is_idle(&self, now: Instant) -> bool {
        !self.is_in_cooldown(now)
            && self
                .failures
                .iter()
                .all(|&t| now.saturating_duration_since(t) >= FAILURE_WINDOW)
    }

    fn prune(&mut self, now: Instant) {
        while let Some(&front) = self.failures.front() {
            if now.saturating_duration_since(front) >= FAILURE_WINDOW {
                self.failures.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> Instant {
        use std::sync::LazyLock;
        static BASE: LazyLock<Instant> = LazyLock::new(Instant::now);
        *BASE + Duration::from_secs(secs)
    }

    #[test]
    fn submit_accepts_answer_case_insensitively() {
        let mut ch = Challenge::new("Ab3De".to_string(), at(0));
        assert!(matches!(ch.submit("ab3de", at(1)), SubmitOutcome::Correct));
        assert!(matches!(ch.submit("AB3DE", at(1)), SubmitOutcome::Correct));
        assert!(matches!(
            ch.submit(" ab3de ", at(1)),
            SubmitOutcome::Correct
        ));
    }

    #[test]
    fn submit_rejects_wrong_answer() {
        let mut ch = Challenge::new("Ab3De".to_string(), at(0));
        let SubmitOutcome::Wrong { invalidated } = ch.submit("xxxxx", at(1)) else {
            panic!("expected Wrong");
        };
        assert!(!invalidated);
        assert_eq!(ch.attempts, 1);
    }

    #[test]
    fn submit_within_ttl_is_not_expired() {
        let mut ch = Challenge::new("Ab3De".to_string(), at(0));
        let now = at(TIME_LIMIT.as_secs());
        assert!(!ch.is_expired(now));
        assert!(matches!(ch.submit("Ab3De", now), SubmitOutcome::Correct));
    }

    #[test]
    fn submit_after_ttl_is_expired() {
        let mut ch = Challenge::new("Ab3De".to_string(), at(0));
        let now = at(TIME_LIMIT.as_secs() + 1);
        assert!(ch.is_expired(now));
        assert!(matches!(ch.submit("Ab3De", now), SubmitOutcome::Expired));
    }

    #[test]
    fn third_wrong_attempt_invalidates_challenge() {
        let mut ch = Challenge::new("Ab3De".to_string(), at(0));
        for (i, expected) in [false, false, true].iter().enumerate() {
            let SubmitOutcome::Wrong { invalidated } = ch.submit("xxxxx", at(1)) else {
                panic!("expected Wrong on attempt {}", i + 1);
            };
            assert_eq!(&invalidated, expected);
        }
    }

    #[test]
    fn six_failures_in_window_trigger_cooldown() {
        let mut ft = FailureTracker::default();
        for i in 0..FAILURE_LIMIT {
            ft.record_failure(at(i as u64));
        }
        assert!(ft.is_in_cooldown(at(FAILURE_LIMIT as u64)));
    }

    #[test]
    fn five_failures_do_not_trigger_cooldown() {
        let mut ft = FailureTracker::default();
        for i in 0..5 {
            ft.record_failure(at(i as u64));
        }
        assert!(!ft.is_in_cooldown(at(5)));
    }

    #[test]
    fn failure_older_than_window_does_not_count() {
        let mut ft = FailureTracker::default();
        // 1回目はウィンドウより古いのでカウントされず、クールダウンは発動しない
        ft.record_failure(at(0));
        for i in 1..FAILURE_LIMIT {
            ft.record_failure(at(FAILURE_WINDOW.as_secs() + 1 + i as u64));
        }
        let now = at(FAILURE_WINDOW.as_secs() + 1 + FAILURE_LIMIT as u64);
        assert!(!ft.is_in_cooldown(now));
    }

    #[test]
    fn cooldown_expires_after_duration() {
        let mut ft = FailureTracker::default();
        for i in 0..FAILURE_LIMIT {
            ft.record_failure(at(i as u64));
        }
        let triggered = at((FAILURE_LIMIT - 1) as u64);
        assert!(ft.is_in_cooldown(triggered + COOLDOWN - Duration::from_secs(1)));
        assert!(!ft.is_in_cooldown(triggered + COOLDOWN));
    }
}
