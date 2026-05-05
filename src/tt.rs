use crate::r#move::Move;

pub const TT_EXACT: u8 = 0;
pub const TT_ALPHA: u8 = 1;
pub const TT_BETA: u8 = 2;

#[derive(Copy, Clone, Default)]
pub struct TTEntry {
    pub key: u64,
    pub best_move: Move,
    pub score: i32,
    pub depth: i16,
    pub flag: u8,
}

pub struct TranspositionTable {
    entries: Vec<TTEntry>,
}

impl TranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        let bytes = size_mb.max(1) * 1024 * 1024;
        let entry_size = std::mem::size_of::<TTEntry>().max(1);
        let count = (bytes / entry_size).max(1);
        Self {
            entries: vec![TTEntry::default(); count],
        }
    }

    pub fn clear(&mut self) {
        self.entries.fill(TTEntry::default());
    }

    pub fn probe(&self, key: u64) -> Option<TTEntry> {
        let entry = self.entries[key as usize % self.entries.len()];
        if entry.key == key {
            Some(entry)
        } else {
            None
        }
    }

    pub fn store(&mut self, key: u64, depth: i32, score: i32, flag: u8, best_move: Move) {
        let idx = key as usize % self.entries.len();
        let existing = self.entries[idx];
        if existing.key != key || depth as i16 >= existing.depth {
            self.entries[idx] = TTEntry {
                key,
                best_move,
                score,
                depth: depth as i16,
                flag,
            };
        }
    }
}
