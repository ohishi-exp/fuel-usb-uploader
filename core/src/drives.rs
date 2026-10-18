//! ドライブ一覧の差分。
//!
//! app は 2 秒おきにリムーバブルドライブの文字の集合を取り、前回の集合と比べて
//! **新しく現れた**ドライブだけを処理する (挿しっぱなしのドライブは 2 回目以降は見ない)。

use std::collections::BTreeSet;

/// ドライブ文字の集合 (大文字の `'A'..='Z'`)。
pub type DriveSet = BTreeSet<char>;

/// `GetLogicalDrives` のビットマスク (bit 0 = A:) をドライブ文字の集合にする。
pub fn from_mask(mask: u32) -> DriveSet {
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| char::from(b'A' + i))
        .collect()
}

/// 前回 `prev` に無く今回 `now` にあるドライブ (昇順)。
pub fn appeared(prev: &DriveSet, now: &DriveSet) -> Vec<char> {
    now.difference(prev).copied().collect()
}

/// ドライブ文字からルートのパス (`E:\`)。
pub fn root(letter: char) -> String {
    format!("{}:\\", letter.to_ascii_uppercase())
}

/// 差分を覚えておく状態。初回の `update` は起動時に挿さっていたドライブも「新しい」として返す
/// (起動前に挿された USB も取り込むため)。
#[derive(Debug, Default)]
pub struct Watcher {
    prev: DriveSet,
}

impl Watcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// 今回の一覧を渡すと、新しく現れたドライブを返し、前回の一覧を差し替える。
    /// 抜かれたドライブは集合から消えるので、挿し直せばまた新しいドライブとして返る。
    pub fn update(&mut self, now: DriveSet) -> Vec<char> {
        let new = appeared(&self.prev, &now);
        self.prev = now;
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(s: &str) -> DriveSet {
        s.chars().collect()
    }

    #[test]
    fn mask_to_letters() {
        assert_eq!(from_mask(0), set(""));
        assert_eq!(from_mask(0b1), set("A"));
        assert_eq!(from_mask(0b1_0100), set("CE"));
        assert_eq!(from_mask(1 << 25), set("Z"));
        // 26 bit より上は無視する
        assert_eq!(from_mask(u32::MAX), set("ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
    }

    #[test]
    fn appeared_is_new_only() {
        assert_eq!(appeared(&set("CD"), &set("CDE")), vec!['E']);
        assert_eq!(appeared(&set("CDE"), &set("CD")), Vec::<char>::new());
        assert_eq!(appeared(&set(""), &set("FE")), vec!['E', 'F']);
        assert_eq!(appeared(&set("E"), &set("E")), Vec::<char>::new());
    }

    #[test]
    fn watcher_reports_reinsert() {
        let mut w = Watcher::new();
        assert_eq!(w.update(set("E")), vec!['E']);
        assert_eq!(w.update(set("E")), Vec::<char>::new());
        assert_eq!(w.update(set("EF")), vec!['F']);
        assert_eq!(w.update(set("F")), Vec::<char>::new());
        assert_eq!(w.update(set("EF")), vec!['E']);
    }

    #[test]
    fn root_path() {
        assert_eq!(root('e'), "E:\\");
        assert_eq!(root('F'), "F:\\");
    }
}
