use crate::r#move::Move;
use crate::zobrist::{bishop_attacks, code_for, piece_key, rook_attacks, AttackTables, ZobristTables};

pub const STANDARD_STARTPOS_FEN: &str =
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
pub const EMPTY: u8 = 0xff;
pub const NO_SQ: u8 = 64;

pub const FILE_A: u64 = 0x0101_0101_0101_0101;
pub const FILE_H: u64 = 0x8080_8080_8080_8080;

pub const CASTLE_WHITE_KING: u8 = 1;
pub const CASTLE_WHITE_QUEEN: u8 = 2;
pub const CASTLE_BLACK_KING: u8 = 4;
pub const CASTLE_BLACK_QUEEN: u8 = 8;

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    pub fn idx(self) -> usize {
        self as usize
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Piece {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
}

impl Piece {
    pub const ALL: [Piece; 6] = [
        Piece::Pawn,
        Piece::Knight,
        Piece::Bishop,
        Piece::Rook,
        Piece::Queen,
        Piece::King,
    ];
}

#[derive(Clone)]
pub struct Board {
    pub pieces: [[u64; 6]; 2],
    pub occupancy: [u64; 3],
    pub squares: [u8; 64],
    pub side_to_move: Color,
    pub castling_rights: u8,
    pub en_passant: u8,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
    pub zobrist_key: u64,
}

#[derive(Copy, Clone)]
pub struct Undo {
    pub captured: u8,
    pub castling_rights: u8,
    pub en_passant: u8,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
    pub zobrist_key: u64,
    pub moved_piece: u8,
}

pub fn piece_code(color: Color, piece: Piece) -> u8 {
    color as u8 * 6 + piece as u8
}

pub fn decode_piece(code: u8) -> Option<(Color, Piece)> {
    if code == EMPTY {
        return None;
    }
    let color = if code < 6 { Color::White } else { Color::Black };
    let piece = match code % 6 {
        0 => Piece::Pawn,
        1 => Piece::Knight,
        2 => Piece::Bishop,
        3 => Piece::Rook,
        4 => Piece::Queen,
        5 => Piece::King,
        _ => return None,
    };
    Some((color, piece))
}

impl Board {
    pub fn empty() -> Self {
        Self {
            pieces: [[0; 6]; 2],
            occupancy: [0; 3],
            squares: [EMPTY; 64],
            side_to_move: Color::White,
            castling_rights: 0,
            en_passant: NO_SQ,
            halfmove_clock: 0,
            fullmove_number: 1,
            zobrist_key: 0,
        }
    }

    pub fn startpos(zobrist: &ZobristTables) -> Self {
        Self::from_fen(STANDARD_STARTPOS_FEN, zobrist).expect("valid startpos")
    }

    pub fn from_fen(fen: &str, zobrist: &ZobristTables) -> Result<Self, String> {
        let mut board = Board::empty();
        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() != 6 {
            return Err(format!("invalid FEN: {fen}"));
        }

        let mut square = 56usize;
        for ch in parts[0].chars() {
            match ch {
                '/' => {
                    if square < 16 {
                        return Err(String::from("invalid FEN ranks"));
                    }
                    square -= 16;
                }
                '1'..='8' => {
                    square += ch.to_digit(10).unwrap() as usize;
                }
                _ => {
                    let (color, piece) = fen_piece(ch).ok_or_else(|| String::from("invalid FEN piece"))?;
                    if square >= 64 {
                        return Err(String::from("invalid FEN square"));
                    }
                    board.add_piece_code(piece_code(color, piece), square as u8, zobrist);
                    square += 1;
                }
            }
        }

        board.side_to_move = match parts[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(String::from("invalid FEN side")),
        };

        board.castling_rights = 0;
        if parts[2] != "-" {
            for ch in parts[2].chars() {
                board.castling_rights |= match ch {
                    'K' => CASTLE_WHITE_KING,
                    'Q' => CASTLE_WHITE_QUEEN,
                    'k' => CASTLE_BLACK_KING,
                    'q' => CASTLE_BLACK_QUEEN,
                    _ => return Err(String::from("invalid FEN castling rights")),
                };
            }
        }

        board.en_passant = if parts[3] == "-" {
            NO_SQ
        } else {
            crate::r#move::square_from_str(parts[3]).ok_or_else(|| String::from("invalid ep square"))?
        };
        board.halfmove_clock = parts[4]
            .parse::<u32>()
            .map_err(|_| String::from("invalid halfmove clock"))?;
        board.fullmove_number = parts[5]
            .parse::<u32>()
            .map_err(|_| String::from("invalid fullmove number"))?;
        board.recompute_occupancy();
        board.zobrist_key = zobrist.hash_board(&board);
        Ok(board)
    }

    pub fn piece_at(&self, square: u8) -> u8 {
        self.squares[square as usize]
    }

    pub fn king_square(&self, color: Color) -> u8 {
        self.pieces[color.idx()][Piece::King as usize].trailing_zeros() as u8
    }

    pub fn occupied(&self) -> u64 {
        self.occupancy[2]
    }

    pub fn make_move(
        &mut self,
        mv: Move,
        attacks: &AttackTables,
        zobrist: &ZobristTables,
    ) -> Option<Undo> {
        let from = mv.from();
        let to = mv.to();
        let moving_code = self.piece_at(from);
        if moving_code == EMPTY {
            return None;
        }
        let (us, moving_piece) = decode_piece(moving_code)?;
        if us != self.side_to_move {
            return None;
        }
        let them = us.opposite();
        let captured = if mv.is_en_passant() {
            code_for(them, Piece::Pawn)
        } else {
            self.piece_at(to)
        };

        let undo = Undo {
            captured,
            castling_rights: self.castling_rights,
            en_passant: self.en_passant,
            halfmove_clock: self.halfmove_clock,
            fullmove_number: self.fullmove_number,
            zobrist_key: self.zobrist_key,
            moved_piece: moving_code,
        };

        if self.en_passant != NO_SQ {
            self.zobrist_key ^= zobrist.en_passant[(self.en_passant & 7) as usize];
        }
        self.zobrist_key ^= zobrist.castling[self.castling_rights as usize];
        self.en_passant = NO_SQ;

        if moving_piece == Piece::Pawn || captured != EMPTY {
            self.halfmove_clock = 0;
        } else {
            self.halfmove_clock += 1;
        }

        if mv.is_en_passant() {
            let capture_square = if us == Color::White { to - 8 } else { to + 8 };
            self.remove_piece_code(code_for(them, Piece::Pawn), capture_square, zobrist);
        } else if captured != EMPTY {
            self.remove_piece_code(captured, to, zobrist);
        }

        self.move_piece_code(moving_code, from, to, zobrist);

        if mv.is_castling() {
            match to {
                6 => self.move_piece_code(code_for(Color::White, Piece::Rook), 7, 5, zobrist),
                2 => self.move_piece_code(code_for(Color::White, Piece::Rook), 0, 3, zobrist),
                62 => self.move_piece_code(code_for(Color::Black, Piece::Rook), 63, 61, zobrist),
                58 => self.move_piece_code(code_for(Color::Black, Piece::Rook), 56, 59, zobrist),
                _ => return None,
            }
        }

        if let Some(promoted) = mv.promotion_piece() {
            self.remove_piece_code(code_for(us, Piece::Pawn), to, zobrist);
            self.add_piece_code(code_for(us, promoted), to, zobrist);
        }

        if mv.is_double_pawn_push() {
            self.en_passant = if us == Color::White { to - 8 } else { to + 8 };
            self.zobrist_key ^= zobrist.en_passant[(self.en_passant & 7) as usize];
        }

        self.castling_rights &= castling_mask(from);
        self.castling_rights &= castling_mask(to);
        self.zobrist_key ^= zobrist.castling[self.castling_rights as usize];

        self.side_to_move = them;
        self.zobrist_key ^= zobrist.side;

        if us == Color::Black {
            self.fullmove_number += 1;
        }

        if self.is_square_attacked(self.king_square(us), them, attacks) {
            self.unmake_move(mv, undo, zobrist);
            return None;
        }

        Some(undo)
    }

    pub fn unmake_move(&mut self, mv: Move, undo: Undo, zobrist: &ZobristTables) {
        let from = mv.from();
        let to = mv.to();
        self.side_to_move = self.side_to_move.opposite();
        let us = self.side_to_move;
        let them = us.opposite();

        if mv.is_castling() {
            match to {
                6 => self.move_piece_code(code_for(Color::White, Piece::Rook), 5, 7, zobrist),
                2 => self.move_piece_code(code_for(Color::White, Piece::Rook), 3, 0, zobrist),
                62 => self.move_piece_code(code_for(Color::Black, Piece::Rook), 61, 63, zobrist),
                58 => self.move_piece_code(code_for(Color::Black, Piece::Rook), 59, 56, zobrist),
                _ => {}
            }
        }

        if mv.is_promotion() {
            let promoted = code_for(us, mv.promotion_piece().unwrap());
            self.remove_piece_code(promoted, to, zobrist);
            self.add_piece_code(code_for(us, Piece::Pawn), from, zobrist);
        } else {
            self.move_piece_code(undo.moved_piece, to, from, zobrist);
        }

        if undo.captured != EMPTY {
            if mv.is_en_passant() {
                let capture_square = if us == Color::White { to - 8 } else { to + 8 };
                self.add_piece_code(code_for(them, Piece::Pawn), capture_square, zobrist);
            } else {
                self.add_piece_code(undo.captured, to, zobrist);
            }
        }

        self.castling_rights = undo.castling_rights;
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.fullmove_number = undo.fullmove_number;
        self.zobrist_key = undo.zobrist_key;
    }

    pub fn is_square_attacked(&self, square: u8, by: Color, attacks: &AttackTables) -> bool {
        let target = 1u64 << square;

        let pawn_attackers = if by == Color::White {
            ((target & !FILE_A) >> 9) | ((target & !FILE_H) >> 7)
        } else {
            ((target & !FILE_H) << 9) | ((target & !FILE_A) << 7)
        };
        if pawn_attackers & self.pieces[by.idx()][Piece::Pawn as usize] != 0 {
            return true;
        }

        if attacks.knight_attacks[square as usize] & self.pieces[by.idx()][Piece::Knight as usize] != 0 {
            return true;
        }

        if attacks.king_attacks[square as usize] & self.pieces[by.idx()][Piece::King as usize] != 0 {
            return true;
        }

        let occupied = self.occupied();
        if bishop_attacks(square, occupied)
            & (self.pieces[by.idx()][Piece::Bishop as usize] | self.pieces[by.idx()][Piece::Queen as usize])
            != 0
        {
            return true;
        }

        rook_attacks(square, occupied)
            & (self.pieces[by.idx()][Piece::Rook as usize] | self.pieces[by.idx()][Piece::Queen as usize])
            != 0
    }

    pub fn in_check(&self, color: Color, attacks: &AttackTables) -> bool {
        self.is_square_attacked(self.king_square(color), color.opposite(), attacks)
    }

    pub fn parse_uci_move(
        &mut self,
        text: &str,
        attacks: &AttackTables,
        zobrist: &ZobristTables,
    ) -> Option<Move> {
        let legal = crate::movegen::generate_legal_moves(self, attacks, zobrist, false);
        legal
            .as_slice()
            .iter()
            .copied()
            .find(|mv| mv.to_uci() == text)
    }

    fn add_piece_code(&mut self, code: u8, square: u8, zobrist: &ZobristTables) {
        let (color, piece) = decode_piece(code).expect("valid piece code");
        let bb = 1u64 << square;
        self.pieces[color.idx()][piece as usize] |= bb;
        self.occupancy[color.idx()] |= bb;
        self.occupancy[2] |= bb;
        self.squares[square as usize] = code;
        self.zobrist_key ^= piece_key(zobrist, code, square);
    }

    fn remove_piece_code(&mut self, code: u8, square: u8, zobrist: &ZobristTables) {
        let (color, piece) = decode_piece(code).expect("valid piece code");
        let bb = 1u64 << square;
        self.pieces[color.idx()][piece as usize] &= !bb;
        self.occupancy[color.idx()] &= !bb;
        self.occupancy[2] &= !bb;
        self.squares[square as usize] = EMPTY;
        self.zobrist_key ^= piece_key(zobrist, code, square);
    }

    fn move_piece_code(&mut self, code: u8, from: u8, to: u8, zobrist: &ZobristTables) {
        self.remove_piece_code(code, from, zobrist);
        self.add_piece_code(code, to, zobrist);
    }

    fn recompute_occupancy(&mut self) {
        self.occupancy[Color::White.idx()] = 0;
        self.occupancy[Color::Black.idx()] = 0;
        for piece in Piece::ALL {
            self.occupancy[Color::White.idx()] |= self.pieces[Color::White.idx()][piece as usize];
            self.occupancy[Color::Black.idx()] |= self.pieces[Color::Black.idx()][piece as usize];
        }
        self.occupancy[2] = self.occupancy[0] | self.occupancy[1];
    }
}

fn fen_piece(ch: char) -> Option<(Color, Piece)> {
    let color = if ch.is_ascii_uppercase() {
        Color::White
    } else {
        Color::Black
    };
    let piece = match ch.to_ascii_lowercase() {
        'p' => Piece::Pawn,
        'n' => Piece::Knight,
        'b' => Piece::Bishop,
        'r' => Piece::Rook,
        'q' => Piece::Queen,
        'k' => Piece::King,
        _ => return None,
    };
    Some((color, piece))
}

fn castling_mask(square: u8) -> u8 {
    match square {
        0 => 0b1101,
        4 => 0b1100,
        7 => 0b1110,
        56 => 0b0111,
        60 => 0b0011,
        63 => 0b1011,
        _ => 0b1111,
    }
}
