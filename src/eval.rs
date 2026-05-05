use crate::board::{Board, Color, Piece};
use crate::zobrist::{bishop_attacks, rook_attacks, AttackTables};

const MATERIAL: [i32; 6] = [100, 320, 330, 500, 900, 0];
const MOBILITY: [i32; 6] = [0, 4, 5, 2, 1, 0];
const DOUBLED_PAWN_PENALTY: i32 = 12;
const ISOLATED_PAWN_PENALTY: i32 = 14;
const PASSED_PAWN_BONUS: [i32; 8] = [0, 8, 16, 28, 44, 64, 90, 0];
const KING_SHIELD_BONUS: i32 = 10;
const KING_OPEN_FILE_PENALTY: i32 = 14;
const KING_SEMI_OPEN_FILE_PENALTY: i32 = 8;
const KING_ZONE_ATTACK_PENALTY: i32 = 6;

const PAWN_PST: [i32; 64] = [
      0,   0,   0,   0,   0,   0,   0,   0,
     10,  10,  10, -20, -20,  10,  10,  10,
      5,  -5, -10,   0,   0, -10,  -5,   5,
      0,   0,   0,  20,  20,   0,   0,   0,
      5,   5,  10,  25,  25,  10,   5,   5,
     10,  10,  20,  30,  30,  20,  10,  10,
     50,  50,  50,  50,  50,  50,  50,  50,
      0,   0,   0,   0,   0,   0,   0,   0,
];
const KNIGHT_PST: [i32; 64] = [
    -50, -40, -30, -30, -30, -30, -40, -50,
    -40, -20,   0,   5,   5,   0, -20, -40,
    -30,   5,  10,  15,  15,  10,   5, -30,
    -30,   0,  15,  20,  20,  15,   0, -30,
    -30,   5,  15,  20,  20,  15,   5, -30,
    -30,   0,  10,  15,  15,  10,   0, -30,
    -40, -20,   0,   0,   0,   0, -20, -40,
    -50, -40, -30, -30, -30, -30, -40, -50,
];
const BISHOP_PST: [i32; 64] = [
    -20, -10, -10, -10, -10, -10, -10, -20,
    -10,   5,   0,   0,   0,   0,   5, -10,
    -10,  10,  10,  10,  10,  10,  10, -10,
    -10,   0,  10,  10,  10,  10,   0, -10,
    -10,   5,   5,  10,  10,   5,   5, -10,
    -10,   0,   5,  10,  10,   5,   0, -10,
    -10,   0,   0,   0,   0,   0,   0, -10,
    -20, -10, -10, -10, -10, -10, -10, -20,
];
const ROOK_PST: [i32; 64] = [
      0,   0,   5,  10,  10,   5,   0,   0,
     -5,   0,   0,   0,   0,   0,   0,  -5,
     -5,   0,   0,   0,   0,   0,   0,  -5,
     -5,   0,   0,   0,   0,   0,   0,  -5,
     -5,   0,   0,   0,   0,   0,   0,  -5,
     -5,   0,   0,   0,   0,   0,   0,  -5,
      5,  10,  10,  10,  10,  10,  10,   5,
      0,   0,   5,  10,  10,   5,   0,   0,
];
const QUEEN_PST: [i32; 64] = [
    -20, -10, -10,  -5,  -5, -10, -10, -20,
    -10,   0,   0,   0,   0,   0,   0, -10,
    -10,   0,   5,   5,   5,   5,   0, -10,
     -5,   0,   5,   5,   5,   5,   0,  -5,
      0,   0,   5,   5,   5,   5,   0,  -5,
    -10,   5,   5,   5,   5,   5,   0, -10,
    -10,   0,   5,   0,   0,   0,   0, -10,
    -20, -10, -10,  -5,  -5, -10, -10, -20,
];
const KING_PST: [i32; 64] = [
     20,  30,  10,   0,   0,  10,  30,  20,
     20,  20,   0,   0,   0,   0,  20,  20,
    -10, -20, -20, -20, -20, -20, -20, -10,
    -20, -30, -30, -40, -40, -30, -30, -20,
    -30, -40, -40, -50, -50, -40, -40, -30,
    -30, -40, -40, -50, -50, -40, -40, -30,
    -30, -40, -40, -50, -50, -40, -40, -30,
    -30, -40, -40, -50, -50, -40, -40, -30,
];

pub fn evaluate(board: &Board, attacks: &AttackTables) -> i32 {
    let mut score = 0i32;
    let occupied = board.occupied();

    for color in [Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        for piece in Piece::ALL {
            let mut bb = board.pieces[color.idx()][piece as usize];
            while bb != 0 {
                let square = bb.trailing_zeros() as u8;
                bb &= bb - 1;
                let idx = if color == Color::White {
                    square as usize
                } else {
                    mirror(square) as usize
                };
                score += sign * MATERIAL[piece as usize];
                score += sign * pst(piece)[idx];
                score += sign * mobility(board, attacks, occupied, color, piece, square);
            }
        }
        score += sign * pawn_structure(board, color);
        score += sign * king_safety(board, attacks, color);
    }

    if board.side_to_move == Color::White {
        score
    } else {
        -score
    }
}

fn mobility(
    board: &Board,
    attacks: &AttackTables,
    occupied: u64,
    color: Color,
    piece: Piece,
    square: u8,
) -> i32 {
    let own = board.occupancy[color.idx()];
    let moves = match piece {
        Piece::Knight => attacks.knight_attacks[square as usize] & !own,
        Piece::Bishop => bishop_attacks(square, occupied) & !own,
        Piece::Rook => rook_attacks(square, occupied) & !own,
        Piece::Queen => (bishop_attacks(square, occupied) | rook_attacks(square, occupied)) & !own,
        _ => 0,
    };
    moves.count_ones() as i32 * MOBILITY[piece as usize]
}

fn pst(piece: Piece) -> &'static [i32; 64] {
    match piece {
        Piece::Pawn => &PAWN_PST,
        Piece::Knight => &KNIGHT_PST,
        Piece::Bishop => &BISHOP_PST,
        Piece::Rook => &ROOK_PST,
        Piece::Queen => &QUEEN_PST,
        Piece::King => &KING_PST,
    }
}

fn mirror(square: u8) -> u8 {
    square ^ 56
}

pub fn piece_value(piece: Piece) -> i32 {
    MATERIAL[piece as usize]
}

fn pawn_structure(board: &Board, color: Color) -> i32 {
    let pawns = board.pieces[color.idx()][Piece::Pawn as usize];
    let enemy_pawns = board.pieces[color.opposite().idx()][Piece::Pawn as usize];
    if pawns == 0 {
        return 0;
    }

    let mut score = 0;
    for file in 0..8 {
        let file_mask = 0x0101_0101_0101_0101u64 << file;
        let count = (pawns & file_mask).count_ones() as i32;
        if count > 1 {
            score -= (count - 1) * DOUBLED_PAWN_PENALTY;
        }
    }

    let mut bb = pawns;
    while bb != 0 {
        let sq = bb.trailing_zeros() as u8;
        bb &= bb - 1;
        let file = sq % 8;

        let left_file = if file > 0 {
            0x0101_0101_0101_0101u64 << (file - 1)
        } else {
            0
        };
        let right_file = if file < 7 {
            0x0101_0101_0101_0101u64 << (file + 1)
        } else {
            0
        };
        if pawns & (left_file | right_file) == 0 {
            score -= ISOLATED_PAWN_PENALTY;
        }

        if is_passed_pawn(sq, color, enemy_pawns) {
            let rank = if color == Color::White { sq / 8 } else { 7 - sq / 8 };
            score += PASSED_PAWN_BONUS[rank as usize];
        }
    }

    score
}

fn is_passed_pawn(square: u8, color: Color, enemy_pawns: u64) -> bool {
    let file = square % 8;
    let rank = square / 8;

    let mut mask = 0u64;
    if color == Color::White {
        for r in (rank + 1)..8 {
            for df in [-1i8, 0, 1] {
                let f = file as i8 + df;
                if (0..8).contains(&f) {
                    let sq = r * 8 + f as u8;
                    mask |= 1u64 << sq;
                }
            }
        }
    } else {
        for r in (0..rank).rev() {
            for df in [-1i8, 0, 1] {
                let f = file as i8 + df;
                if (0..8).contains(&f) {
                    let sq = r * 8 + f as u8;
                    mask |= 1u64 << sq;
                }
            }
        }
    }

    enemy_pawns & mask == 0
}

fn king_safety(board: &Board, attacks: &AttackTables, color: Color) -> i32 {
    let king_sq = board.king_square(color);
    let king_file = king_sq % 8;
    let king_zone = attacks.king_attacks[king_sq as usize] | (1u64 << king_sq);
    let own_pawns = board.pieces[color.idx()][Piece::Pawn as usize];
    let enemy = color.opposite();
    let occupied = board.occupied();

    let mut score = 0;

    if let Some(shield_rank) = if color == Color::White {
        (king_sq / 8).checked_add(1).filter(|&r| r < 8)
    } else {
        (king_sq / 8).checked_sub(1)
    } {
        for df in [-1i8, 0, 1] {
            let f = king_file as i8 + df;
            if !(0..8).contains(&f) {
                continue;
            }
            let sq = shield_rank * 8 + f as u8;
            if own_pawns & (1u64 << sq) != 0 {
                score += KING_SHIELD_BONUS;
            }
        }
    }

    for df in [-1i8, 0, 1] {
        let f = king_file as i8 + df;
        if !(0..8).contains(&f) {
            continue;
        }
        let file_mask = 0x0101_0101_0101_0101u64 << f;
        let own_pawn_on_file = own_pawns & file_mask != 0;
        let enemy_pawn_on_file = board.pieces[enemy.idx()][Piece::Pawn as usize] & file_mask != 0;
        if !own_pawn_on_file && !enemy_pawn_on_file {
            score -= KING_OPEN_FILE_PENALTY;
        } else if !own_pawn_on_file {
            score -= KING_SEMI_OPEN_FILE_PENALTY;
        }
    }

    let mut attackers = 0;

    let mut bb = board.pieces[enemy.idx()][Piece::Knight as usize];
    while bb != 0 {
        let sq = bb.trailing_zeros() as u8;
        bb &= bb - 1;
        if attacks.knight_attacks[sq as usize] & king_zone != 0 {
            attackers += 2;
        }
    }

    bb = board.pieces[enemy.idx()][Piece::Bishop as usize];
    while bb != 0 {
        let sq = bb.trailing_zeros() as u8;
        bb &= bb - 1;
        if bishop_attacks(sq, occupied) & king_zone != 0 {
            attackers += 2;
        }
    }

    bb = board.pieces[enemy.idx()][Piece::Rook as usize];
    while bb != 0 {
        let sq = bb.trailing_zeros() as u8;
        bb &= bb - 1;
        if rook_attacks(sq, occupied) & king_zone != 0 {
            attackers += 3;
        }
    }

    bb = board.pieces[enemy.idx()][Piece::Queen as usize];
    while bb != 0 {
        let sq = bb.trailing_zeros() as u8;
        bb &= bb - 1;
        if (rook_attacks(sq, occupied) | bishop_attacks(sq, occupied)) & king_zone != 0 {
            attackers += 5;
        }
    }

    let king_bb = 1u64 << king_sq;
    let pawn_attackers = if enemy == Color::White {
        ((king_bb & !crate::board::FILE_A) >> 9) | ((king_bb & !crate::board::FILE_H) >> 7)
    } else {
        ((king_bb & !crate::board::FILE_H) << 9) | ((king_bb & !crate::board::FILE_A) << 7)
    };
    if pawn_attackers & board.pieces[enemy.idx()][Piece::Pawn as usize] != 0 {
        attackers += 1;
    }

    score - attackers * KING_ZONE_ATTACK_PENALTY
}
