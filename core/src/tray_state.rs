//! トレイのアイコンが表す状態 (色とツールチップ) と、同時に走る worker からの状態の集約。

use crate::outcome::Outcome;

/// トレイの状態。WPARAM で main thread に運ぶので `u8` に落とせる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrayState {
    /// 待機 (起動直後)
    Idle = 0,
    /// 読み取り・送信中
    Busy = 1,
    /// 取り込み済み
    Done = 2,
    /// 失敗
    Failed = 3,
}

impl TrayState {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Idle),
            1 => Some(Self::Busy),
            2 => Some(Self::Done),
            3 => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Self::Idle => "給油データ USB アップローダー",
            Self::Busy => "給油データ: 読み取り・送信中",
            Self::Done => "給油データ: 取り込み済み (USB は抜けます)",
            Self::Failed => "給油データ: 失敗 (通知を確認してください)",
        }
    }
}

/// 1 回の取り込みの結果を状態にする (`Imported` だけが Done)。
pub fn from_outcome(outcome: &Outcome) -> TrayState {
    match outcome {
        Outcome::Imported { .. } => TrayState::Done,
        _ => TrayState::Failed,
    }
}

/// 走っている worker の数と最後の結果。1 本でも走っていれば Busy のまま、全部終わったら最後の結果を出す。
#[derive(Debug)]
pub struct Tracker {
    active: usize,
    last: TrayState,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            active: 0,
            last: TrayState::Idle,
        }
    }

    /// worker が 1 本始まった。
    pub fn begin(&mut self) -> TrayState {
        self.active += 1;
        TrayState::Busy
    }

    /// worker が 1 本終わった。全部終わったら表示する状態を返す (`None` はまだ Busy のまま)。
    pub fn end(&mut self, result: TrayState) -> Option<TrayState> {
        self.active = self.active.saturating_sub(1);
        self.last = result;
        (self.active == 0).then_some(self.last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_outcome() {
        assert_eq!(
            super::from_outcome(&Outcome::Imported {
                inserted: 1,
                skipped: 0,
                latest_start: None
            }),
            TrayState::Done
        );
        assert_eq!(
            super::from_outcome(&Outcome::BadCsv {
                error: "csv".into(),
                line: None,
                reason: None
            }),
            TrayState::Failed
        );
    }

    #[test]
    fn tooltips() {
        assert_eq!(TrayState::Idle.tooltip(), "給油データ USB アップローダー");
        assert_eq!(TrayState::Busy.tooltip(), "給油データ: 読み取り・送信中");
        assert_eq!(
            TrayState::Done.tooltip(),
            "給油データ: 取り込み済み (USB は抜けます)"
        );
        assert_eq!(
            TrayState::Failed.tooltip(),
            "給油データ: 失敗 (通知を確認してください)"
        );
    }

    #[test]
    fn u8_round_trip() {
        for s in [
            TrayState::Idle,
            TrayState::Busy,
            TrayState::Done,
            TrayState::Failed,
        ] {
            assert_eq!(TrayState::from_u8(s as u8), Some(s));
        }
        assert_eq!(TrayState::from_u8(4), None);
    }

    #[test]
    fn tracker_single() {
        let mut t = Tracker::new();
        assert_eq!(t.begin(), TrayState::Busy);
        assert_eq!(t.end(TrayState::Done), Some(TrayState::Done));
    }

    #[test]
    fn tracker_overlapping() {
        let mut t = Tracker::new();
        t.begin();
        t.begin();
        assert_eq!(t.end(TrayState::Done), None);
        assert_eq!(t.end(TrayState::Failed), Some(TrayState::Failed));
    }

    #[test]
    fn tracker_sequential() {
        let mut t = Tracker::new();
        t.begin();
        assert_eq!(t.end(TrayState::Failed), Some(TrayState::Failed));
        t.begin();
        assert_eq!(t.end(TrayState::Done), Some(TrayState::Done));
    }
}
