use crate::board::{piece_code, Board, Color, Piece, NO_SQ};

pub struct AttackTables {
    pub pawn_attacks: [[u64; 64]; 2],
    pub knight_attacks: [u64; 64],
    pub king_attacks: [u64; 64],
}

impl AttackTables {
    pub fn new() -> Self {
        let mut pawn_attacks = [[0u64; 64]; 2];
        let mut knight_attacks = [0u64; 64];
        let mut king_attacks = [0u64; 64];

        for square in 0..64 {
            pawn_attacks[Color::White.idx()][square] = white_pawn_attacks(square as u8);
            pawn_attacks[Color::Black.idx()][square] = black_pawn_attacks(square as u8);
            knight_attacks[square] = knight_attack_mask(square as u8);
            king_attacks[square] = king_attack_mask(square as u8);
        }

        Self {
            pawn_attacks,
            knight_attacks,
            king_attacks,
        }
    }
}

pub struct ZobristTables {
    pub pieces: [[[u64; 64]; 6]; 2],
    pub castling: [u64; 16],
    pub en_passant: [u64; 8],
    pub side: u64,
}

impl ZobristTables {
    pub fn new() -> Self {
        let mut rng = SplitMix64::new(0x9e37_79b9_7f4a_7c15);
        let mut pieces = [[[0u64; 64]; 6]; 2];
        let mut castling = [0u64; 16];
        let mut en_passant = [0u64; 8];

        for color in 0..2 {
            for piece in 0..6 {
                for square in 0..64 {
                    pieces[color][piece][square] = rng.next();
                }
            }
        }
        for key in &mut castling {
            *key = rng.next();
        }
        for key in &mut en_passant {
            *key = rng.next();
        }

        Self {
            pieces,
            castling,
            en_passant,
            side: rng.next(),
        }
    }

    pub fn hash_board(&self, board: &Board) -> u64 {
        let mut key = 0u64;
        for square in 0..64 {
            let code = board.squares[square];
            if code == crate::board::EMPTY {
                continue;
            }
            let color = (code / 6) as usize;
            let piece = (code % 6) as usize;
            key ^= self.pieces[color][piece][square];
        }
        key ^= self.castling[board.castling_rights as usize];
        if board.en_passant != NO_SQ {
            key ^= self.en_passant[(board.en_passant & 7) as usize];
        }
        if board.side_to_move == Color::Black {
            key ^= self.side;
        }
        key
    }
}

#[derive(Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

fn white_pawn_attacks(square: u8) -> u64 {
    let bb = 1u64 << square;
    ((bb & !crate::board::FILE_A) << 7) | ((bb & !crate::board::FILE_H) << 9)
}

fn black_pawn_attacks(square: u8) -> u64 {
    let bb = 1u64 << square;
    ((bb & !crate::board::FILE_H) >> 7) | ((bb & !crate::board::FILE_A) >> 9)
}

fn knight_attack_mask(square: u8) -> u64 {
    let rank = (square / 8) as i32;
    let file = (square % 8) as i32;
    let deltas = [
        (2, 1),
        (2, -1),
        (1, 2),
        (1, -2),
        (-1, 2),
        (-1, -2),
        (-2, 1),
        (-2, -1),
    ];
    let mut attacks = 0u64;

    for (dr, df) in deltas {
        let nr = rank + dr;
        let nf = file + df;
        if (0..8).contains(&nr) && (0..8).contains(&nf) {
            attacks |= 1u64 << (nr * 8 + nf);
        }
    }
    attacks
}

fn king_attack_mask(square: u8) -> u64 {
    let rank = (square / 8) as i32;
    let file = (square % 8) as i32;
    let mut attacks = 0u64;

    for dr in -1..=1 {
        for df in -1..=1 {
            if dr == 0 && df == 0 {
                continue;
            }
            let nr = rank + dr;
            let nf = file + df;
            if (0..8).contains(&nr) && (0..8).contains(&nf) {
                attacks |= 1u64 << (nr * 8 + nf);
            }
        }
    }
    attacks
}

pub fn rook_attacks(square: u8, occupied: u64) -> u64 {
    ray_attacks(square, occupied, &[(1, 0), (-1, 0), (0, 1), (0, -1)])
}

pub fn bishop_attacks(square: u8, occupied: u64) -> u64 {
    ray_attacks(square, occupied, &[(1, 1), (1, -1), (-1, 1), (-1, -1)])
}

fn ray_attacks(square: u8, occupied: u64, directions: &[(i32, i32)]) -> u64 {
    let rank = (square / 8) as i32;
    let file = (square % 8) as i32;
    let mut attacks = 0u64;

    for (dr, df) in directions {
        let mut nr = rank + dr;
        let mut nf = file + df;
        while (0..8).contains(&nr) && (0..8).contains(&nf) {
            let sq = (nr * 8 + nf) as u8;
            let bb = 1u64 << sq;
            attacks |= bb;
            if occupied & bb != 0 {
                break;
            }
            nr += dr;
            nf += df;
        }
    }

    attacks
}

pub fn piece_key(zobrist: &ZobristTables, code: u8, square: u8) -> u64 {
    let color = (code / 6) as usize;
    let piece = (code % 6) as usize;
    zobrist.pieces[color][piece][square as usize]
}

pub fn code_for(color: Color, piece: Piece) -> u8 {
    piece_code(color, piece)
}
