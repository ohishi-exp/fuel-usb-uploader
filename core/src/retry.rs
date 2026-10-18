//! やり直しの方針: 一時的な失敗 (503・通信失敗) だけ、最大 3 回、間隔 5・15・45 秒。

use std::time::Duration;

use crate::outcome::Outcome;

/// やり直しの前に待つ時間 (n 回目のやり直しの前に `DELAYS[n]`)。
pub const DELAYS: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(45),
];

/// `retries_done` 回やり直した後の結果が `outcome` だったとき、次のやり直しの前に待つ時間。
/// やり直さないなら `None`。
pub fn next_delay(outcome: &Outcome, retries_done: usize) -> Option<Duration> {
    if !outcome.is_transient() {
        return None;
    }
    DELAYS.get(retries_done).copied()
}

/// `send` を方針どおりに呼ぶ。`sleep` は待ち方 (テストでは記録だけする)。
pub fn run<S, W>(mut send: S, mut sleep: W) -> Outcome
where
    S: FnMut() -> Outcome,
    W: FnMut(Duration),
{
    let mut retries = 0;
    loop {
        let outcome = send();
        match next_delay(&outcome, retries) {
            Some(d) => {
                sleep(d);
                retries += 1;
            }
            None => return outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::Transient;

    fn imported() -> Outcome {
        Outcome::Imported {
            inserted: 1,
            skipped: 0,
            latest_start: None,
        }
    }

    #[test]
    fn only_transient_retries() {
        assert_eq!(next_delay(&imported(), 0), None);
        assert_eq!(next_delay(&Outcome::Unauthorized, 0), None);
        assert_eq!(next_delay(&Outcome::Unknown { status: 500 }, 0), None);
        assert_eq!(
            next_delay(&Outcome::network(), 0),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            next_delay(&Outcome::Transient(Transient::Unavailable), 1),
            Some(Duration::from_secs(15))
        );
        assert_eq!(
            next_delay(&Outcome::network(), 2),
            Some(Duration::from_secs(45))
        );
        assert_eq!(next_delay(&Outcome::network(), 3), None);
    }

    #[test]
    fn gives_up_after_three_retries() {
        let mut calls = 0;
        let mut waits = Vec::new();
        let out = run(
            || {
                calls += 1;
                Outcome::network()
            },
            |d| waits.push(d.as_secs()),
        );
        assert_eq!(out, Outcome::network());
        assert_eq!(calls, 4, "最初の 1 回 + やり直し 3 回");
        assert_eq!(waits, vec![5, 15, 45]);
    }

    #[test]
    fn stops_on_success() {
        let mut calls = 0;
        let mut waits = Vec::new();
        let out = run(
            || {
                calls += 1;
                if calls < 3 {
                    Outcome::Transient(Transient::Unavailable)
                } else {
                    imported()
                }
            },
            |d| waits.push(d.as_secs()),
        );
        assert_eq!(out, imported());
        assert_eq!(calls, 3);
        assert_eq!(waits, vec![5, 15]);
    }

    #[test]
    fn no_retry_on_permanent_error() {
        let mut calls = 0;
        let out = run(
            || {
                calls += 1;
                Outcome::Unauthorized
            },
            |_| panic!("待たない"),
        );
        assert_eq!(out, Outcome::Unauthorized);
        assert_eq!(calls, 1);
    }
}
