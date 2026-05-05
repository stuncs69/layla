use crate::board::{
    Board, Color, Piece, CASTLE_BLACK_KING, CASTLE_BLACK_QUEEN, CASTLE_WHITE_KING,
    CASTLE_WHITE_QUEEN, EMPTY,
};
use crate::r#move::{
    Move, MoveList, FLAG_CAPTURE, FLAG_CASTLING, FLAG_DOUBLE_PAWN_PUSH, FLAG_EN_PASSANT,
};
use crate::zobrist::{bishop_attacks, rook_attacks, AttackTables, ZobristTables};

pub fn generate_legal_moves(
    board: &mut Board,
    attacks: &AttackTables,
    zobrist: &ZobristTables,
    captures_only: bool,
) -> MoveList {
    let pseudo = generate_pseudo_legal_moves(board, attacks, captures_only);
    let mut legal = MoveList::default();
    for mv in pseudo.as_slice() {
        if let Some(undo) = board.make_move(*mv, attacks, zobrist) {
            legal.push(*mv);
            board.unmake_move(*mv, undo, zobrist);
        }
    }
    legal
}

pub fn generate_pseudo_legal_moves(
    board: &Board,
    attacks: &AttackTables,
    captures_only: bool,
) -> MoveList {
    let mut list = MoveList::default();
    let us = board.side_to_move;
    let them = us.opposite();
    let own_occ = board.occupancy[us.idx()];
    let opp_occ = board.occupancy[them.idx()];
    let occupied = board.occupied();

    generate_pawns(board, attacks, captures_only, &mut list);

    let mut knights = board.pieces[us.idx()][Piece::Knight as usize];
    while knights != 0 {
        let from = pop_lsb(&mut knights);
        let mut targets = attacks.knight_attacks[from as usize] & !own_occ;
        if captures_only {
            targets &= opp_occ;
        }
        push_targets(board, from, targets, &mut list);
    }

    let mut bishops = board.pieces[us.idx()][Piece::Bishop as usize];
    while bishops != 0 {
        let from = pop_lsb(&mut bishops);
        let mut targets = bishop_attacks(from, occupied) & !own_occ;
        if captures_only {
            targets &= opp_occ;
        }
        push_targets(board, from, targets, &mut list);
    }

    let mut rooks = board.pieces[us.idx()][Piece::Rook as usize];
    while rooks != 0 {
        let from = pop_lsb(&mut rooks);
        let mut targets = rook_attacks(from, occupied) & !own_occ;
        if captures_only {
            targets &= opp_occ;
        }
        push_targets(board, from, targets, &mut list);
    }

    let mut queens = board.pieces[us.idx()][Piece::Queen as usize];
    while queens != 0 {
        let from = pop_lsb(&mut queens);
        let mut targets = (rook_attacks(from, occupied) | bishop_attacks(from, occupied)) & !own_occ;
        if captures_only {
            targets &= opp_occ;
        }
        push_targets(board, from, targets, &mut list);
    }

    let king = board.king_square(us);
    let mut king_targets = attacks.king_attacks[king as usize] & !own_occ;
    if captures_only {
        king_targets &= opp_occ;
    }
    push_targets(board, king, king_targets, &mut list);

    if !captures_only && !board.in_check(us, attacks) {
        if us == Color::White {
            if board.castling_rights & CASTLE_WHITE_KING != 0
                && board.piece_at(7) == crate::board::piece_code(Color::White, Piece::Rook)
                && occupied & ((1u64 << 5) | (1u64 << 6)) == 0
                && !board.is_square_attacked(5, them, attacks)
                && !board.is_square_attacked(6, them, attacks)
            {
                list.push(Move::new(4, 6, 0, FLAG_CASTLING));
            }
            if board.castling_rights & CASTLE_WHITE_QUEEN != 0
                && board.piece_at(0) == crate::board::piece_code(Color::White, Piece::Rook)
                && occupied & ((1u64 << 1) | (1u64 << 2) | (1u64 << 3)) == 0
                && !board.is_square_attacked(3, them, attacks)
                && !board.is_square_attacked(2, them, attacks)
            {
                list.push(Move::new(4, 2, 0, FLAG_CASTLING));
            }
        } else {
            if board.castling_rights & CASTLE_BLACK_KING != 0
                && board.piece_at(63) == crate::board::piece_code(Color::Black, Piece::Rook)
                && occupied & ((1u64 << 61) | (1u64 << 62)) == 0
                && !board.is_square_attacked(61, them, attacks)
                && !board.is_square_attacked(62, them, attacks)
            {
                list.push(Move::new(60, 62, 0, FLAG_CASTLING));
            }
            if board.castling_rights & CASTLE_BLACK_QUEEN != 0
                && board.piece_at(56) == crate::board::piece_code(Color::Black, Piece::Rook)
                && occupied & ((1u64 << 57) | (1u64 << 58) | (1u64 << 59)) == 0
                && !board.is_square_attacked(59, them, attacks)
                && !board.is_square_attacked(58, them, attacks)
            {
                list.push(Move::new(60, 58, 0, FLAG_CASTLING));
            }
        }
    }

    list
}

fn generate_pawns(board: &Board, attacks: &AttackTables, captures_only: bool, list: &mut MoveList) {
    let us = board.side_to_move;
    let them = us.opposite();
    let occupied = board.occupied();
    let opp_occ = board.occupancy[them.idx()];
    let mut pawns = board.pieces[us.idx()][Piece::Pawn as usize];

    while pawns != 0 {
        let from = pop_lsb(&mut pawns);
        let rank = from / 8;
        let promotion_rank = if us == Color::White { 6 } else { 1 };
        let start_rank = if us == Color::White { 1 } else { 6 };

        if !captures_only {
            let to = if us == Color::White { from + 8 } else { from - 8 };
            if occupied & (1u64 << to) == 0 {
                if rank == promotion_rank {
                    push_promotions(list, from, to, 0);
                } else {
                    list.push(Move::new(from, to, 0, 0));
                    if rank == start_rank {
                        let to2 = if us == Color::White { from + 16 } else { from - 16 };
                        if occupied & (1u64 << to2) == 0 {
                            list.push(Move::new(from, to2, 0, FLAG_DOUBLE_PAWN_PUSH));
                        }
                    }
                }
            }
        }

        let mut captures = attacks.pawn_attacks[us.idx()][from as usize] & opp_occ;
        while captures != 0 {
            let to = pop_lsb(&mut captures);
            if rank == promotion_rank {
                push_promotions(list, from, to, FLAG_CAPTURE);
            } else {
                list.push(Move::new(from, to, 0, FLAG_CAPTURE));
            }
        }

        if board.en_passant != crate::board::NO_SQ
            && attacks.pawn_attacks[us.idx()][from as usize] & (1u64 << board.en_passant) != 0
        {
            list.push(Move::new(from, board.en_passant, 0, FLAG_CAPTURE | FLAG_EN_PASSANT));
        }
    }
}

fn push_promotions(list: &mut MoveList, from: u8, to: u8, flags: u8) {
    list.push(Move::new(from, to, 1, flags));
    list.push(Move::new(from, to, 2, flags));
    list.push(Move::new(from, to, 3, flags));
    list.push(Move::new(from, to, 4, flags));
}

fn push_targets(board: &Board, from: u8, mut targets: u64, list: &mut MoveList) {
    while targets != 0 {
        let to = pop_lsb(&mut targets);
        let capture = board.piece_at(to) != EMPTY;
        list.push(Move::new(from, to, 0, if capture { FLAG_CAPTURE } else { 0 }));
    }
}

pub fn pop_lsb(bb: &mut u64) -> u8 {
    let square = bb.trailing_zeros() as u8;
    *bb &= *bb - 1;
    square
}
