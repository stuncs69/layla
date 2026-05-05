use crate::board::Piece;

pub const FLAG_CAPTURE: u8 = 1 << 0;
pub const FLAG_DOUBLE_PAWN_PUSH: u8 = 1 << 1;
pub const FLAG_EN_PASSANT: u8 = 1 << 2;
pub const FLAG_CASTLING: u8 = 1 << 3;

#[derive(Copy, Clone, Eq, PartialEq, Default, Debug)]
pub struct Move(pub u32);

impl Move {
    pub const NULL: Self = Self(0);

    pub const fn new(from: u8, to: u8, promotion: u8, flags: u8) -> Self {
        Self(
            (from as u32)
                | ((to as u32) << 6)
                | ((promotion as u32) << 12)
                | ((flags as u32) << 15),
        )
    }

    pub const fn from(self) -> u8 {
        (self.0 & 0x3f) as u8
    }

    pub const fn to(self) -> u8 {
        ((self.0 >> 6) & 0x3f) as u8
    }

    pub const fn promotion(self) -> u8 {
        ((self.0 >> 12) & 0x7) as u8
    }

    pub const fn flags(self) -> u8 {
        ((self.0 >> 15) & 0x1f) as u8
    }

    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    pub const fn is_capture(self) -> bool {
        self.flags() & FLAG_CAPTURE != 0
    }

    pub const fn is_en_passant(self) -> bool {
        self.flags() & FLAG_EN_PASSANT != 0
    }

    pub const fn is_castling(self) -> bool {
        self.flags() & FLAG_CASTLING != 0
    }

    pub const fn is_double_pawn_push(self) -> bool {
        self.flags() & FLAG_DOUBLE_PAWN_PUSH != 0
    }

    pub const fn is_promotion(self) -> bool {
        self.promotion() != 0
    }

    pub const fn promotion_piece(self) -> Option<Piece> {
        match self.promotion() {
            1 => Some(Piece::Knight),
            2 => Some(Piece::Bishop),
            3 => Some(Piece::Rook),
            4 => Some(Piece::Queen),
            _ => None,
        }
    }

    pub fn to_uci(self) -> String {
        if self.is_null() {
            return String::from("0000");
        }

        let mut out = String::with_capacity(5);
        out.push(file_char(self.from()));
        out.push(rank_char(self.from()));
        out.push(file_char(self.to()));
        out.push(rank_char(self.to()));
        if let Some(piece) = self.promotion_piece() {
            out.push(match piece {
                Piece::Knight => 'n',
                Piece::Bishop => 'b',
                Piece::Rook => 'r',
                Piece::Queen => 'q',
                _ => unreachable!(),
            });
        }
        out
    }
}

pub fn square_from_str(s: &str) -> Option<u8> {
    let bytes = s.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let file = bytes[0];
    let rank = bytes[1];
    if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
        return None;
    }
    Some((rank - b'1') * 8 + (file - b'a'))
}

fn file_char(square: u8) -> char {
    (b'a' + (square & 7)) as char
}

fn rank_char(square: u8) -> char {
    (b'1' + (square >> 3)) as char
}

#[derive(Clone)]
pub struct MoveList {
    moves: [Move; 256],
    len: usize,
}

impl Default for MoveList {
    fn default() -> Self {
        Self {
            moves: [Move::NULL; 256],
            len: 0,
        }
    }
}

impl MoveList {
    pub fn push(&mut self, mv: Move) {
        self.moves[self.len] = mv;
        self.len += 1;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[Move] {
        &self.moves[..self.len]
    }

    pub fn as_mut_slice(&mut self) -> &mut [Move] {
        &mut self.moves[..self.len]
    }
}
