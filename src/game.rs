use crate::board::{decode_piece, Board, Color, Piece};
use crate::movegen::generate_legal_moves;
use crate::r#move::Move;
use crate::zobrist::{AttackTables, ZobristTables};

#[derive(Clone, Debug)]
pub struct Played {
    pub mv: Move,
    pub san: String,
    pub color: Color,
    pub piece: Piece,
    pub captured: Option<Piece>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Checkmate { winner: Color },
    Stalemate,
    Repetition,
    FiftyMoves,
    InsufficientMaterial,
}

pub struct Game {
    attacks: AttackTables,
    zobrist: ZobristTables,
    board: Board,
    keys: Vec<u64>,
    played: Vec<Played>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        let zobrist = ZobristTables::new();
        let board = Board::startpos(&zobrist);
        Self::with_board(zobrist, board)
    }

    pub fn from_fen(fen: &str) -> Result<Self, String> {
        let zobrist = ZobristTables::new();
        let board = Board::from_fen(fen, &zobrist)?;
        Ok(Self::with_board(zobrist, board))
    }

    fn with_board(zobrist: ZobristTables, board: Board) -> Self {
        Self {
            attacks: AttackTables::new(),
            zobrist,
            board,
            keys: Vec::new(),
            played: Vec::new(),
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn side_to_move(&self) -> Color {
        self.board.side_to_move
    }

    pub fn piece_at(&self, square: u8) -> Option<(Color, Piece)> {
        decode_piece(self.board.piece_at(square))
    }

    pub fn legal_moves(&mut self) -> Vec<Move> {
        generate_legal_moves(&mut self.board, &self.attacks, &self.zobrist, false)
            .as_slice()
            .to_vec()
    }

    pub fn play(&mut self, mv: Move) -> Result<&Played, String> {
        let legal = generate_legal_moves(&mut self.board, &self.attacks, &self.zobrist, false);
        if !legal.as_slice().contains(&mv) {
            return Err(format!("illegal move: {}", mv.to_uci()));
        }

        let (color, piece) = decode_piece(self.board.piece_at(mv.from()))
            .ok_or_else(|| format!("no piece on move origin: {}", mv.to_uci()))?;
        let captured = if mv.is_en_passant() {
            Some(Piece::Pawn)
        } else {
            decode_piece(self.board.piece_at(mv.to())).map(|(_, piece)| piece)
        };
        let san = san(&mut self.board, mv, &self.attacks, &self.zobrist);

        let key = self.board.zobrist_key;
        self.board
            .make_move(mv, &self.attacks, &self.zobrist)
            .ok_or_else(|| format!("failed to make move: {}", mv.to_uci()))?;
        self.keys.push(key);
        self.played.push(Played {
            mv,
            san,
            color,
            piece,
            captured,
        });
        Ok(self.played.last().expect("move was just recorded"))
    }

    pub fn in_check(&self) -> bool {
        self.board.in_check(self.board.side_to_move, &self.attacks)
    }

    pub fn king_square(&self, color: Color) -> u8 {
        self.board.king_square(color)
    }

    pub fn outcome(&mut self) -> Option<Outcome> {
        let legal = generate_legal_moves(&mut self.board, &self.attacks, &self.zobrist, false);
        if legal.is_empty() {
            return Some(if self.in_check() {
                Outcome::Checkmate {
                    winner: self.board.side_to_move.opposite(),
                }
            } else {
                Outcome::Stalemate
            });
        }
        if self.board.halfmove_clock >= 100 {
            return Some(Outcome::FiftyMoves);
        }
        let key = self.board.zobrist_key;
        if self.keys.iter().filter(|&&k| k == key).count() + 1 >= 3 {
            return Some(Outcome::Repetition);
        }
        if insufficient_material(&self.board) {
            return Some(Outcome::InsufficientMaterial);
        }
        None
    }

    pub fn history(&self) -> &[u64] {
        &self.keys
    }

    pub fn moves(&self) -> &[Played] {
        &self.played
    }
}

fn insufficient_material(board: &Board) -> bool {
    let heavy_or_pawn = |color: Color| {
        let pieces = &board.pieces[color.idx()];
        pieces[Piece::Pawn as usize] | pieces[Piece::Rook as usize] | pieces[Piece::Queen as usize]
    };
    if heavy_or_pawn(Color::White) | heavy_or_pawn(Color::Black) != 0 {
        return false;
    }

    let knights = |color: Color| board.pieces[color.idx()][Piece::Knight as usize];
    let bishops = |color: Color| board.pieces[color.idx()][Piece::Bishop as usize];
    let minors = (knights(Color::White) | bishops(Color::White) | knights(Color::Black)
        | bishops(Color::Black))
    .count_ones();
    if minors <= 1 {
        return true;
    }

    let (white_bishops, black_bishops) = (bishops(Color::White), bishops(Color::Black));
    if minors == 2 && white_bishops.count_ones() == 1 && black_bishops.count_ones() == 1 {
        let square_color = |bb: u64| {
            let square = bb.trailing_zeros();
            ((square & 7) + (square >> 3)) & 1
        };
        return square_color(white_bishops) == square_color(black_bishops);
    }
    false
}

pub fn san(board: &mut Board, mv: Move, attacks: &AttackTables, zobrist: &ZobristTables) -> String {
    let mut out = String::with_capacity(8);
    let (color, piece) = decode_piece(board.piece_at(mv.from())).expect("piece on move origin");

    if mv.is_castling() {
        out.push_str(if mv.to() & 7 == 6 { "O-O" } else { "O-O-O" });
    } else if piece == Piece::Pawn {
        if mv.is_capture() {
            out.push(file_char(mv.from()));
            out.push('x');
        }
        push_square(&mut out, mv.to());
        if let Some(promoted) = mv.promotion_piece() {
            out.push('=');
            out.push(piece_letter(promoted));
        }
    } else {
        out.push(piece_letter(piece));
        let legal = generate_legal_moves(board, attacks, zobrist, false);
        let rivals: Vec<u8> = legal
            .as_slice()
            .iter()
            .filter(|other| {
                other.to() == mv.to()
                    && other.from() != mv.from()
                    && decode_piece(board.piece_at(other.from())) == Some((color, piece))
            })
            .map(|other| other.from())
            .collect();
        if !rivals.is_empty() {
            let shares_file = rivals.iter().any(|&sq| sq & 7 == mv.from() & 7);
            let shares_rank = rivals.iter().any(|&sq| sq >> 3 == mv.from() >> 3);
            if !shares_file {
                out.push(file_char(mv.from()));
            } else if !shares_rank {
                out.push(rank_char(mv.from()));
            } else {
                push_square(&mut out, mv.from());
            }
        }
        if mv.is_capture() {
            out.push('x');
        }
        push_square(&mut out, mv.to());
    }

    let undo = board
        .make_move(mv, attacks, zobrist)
        .expect("san requires a legal move");
    if board.in_check(board.side_to_move, attacks) {
        let mated = generate_legal_moves(board, attacks, zobrist, false).is_empty();
        out.push(if mated { '#' } else { '+' });
    }
    board.unmake_move(mv, undo, zobrist);
    out
}

fn piece_letter(piece: Piece) -> char {
    match piece {
        Piece::Pawn => 'P',
        Piece::Knight => 'N',
        Piece::Bishop => 'B',
        Piece::Rook => 'R',
        Piece::Queen => 'Q',
        Piece::King => 'K',
    }
}

fn file_char(square: u8) -> char {
    (b'a' + (square & 7)) as char
}

fn rank_char(square: u8) -> char {
    (b'1' + (square >> 3)) as char
}

fn push_square(out: &mut String, square: u8) {
    out.push(file_char(square));
    out.push(rank_char(square));
}

#[cfg(test)]
mod tests {
    use super::{Game, Outcome};
    use crate::board::{Color, Piece};

    fn play(game: &mut Game, text: &str) -> String {
        let mv = *game
            .legal_moves()
            .iter()
            .find(|mv| mv.to_uci() == text)
            .unwrap_or_else(|| panic!("{text} is not legal"));
        game.play(mv).unwrap().san.clone()
    }

    fn san_of(fen: &str, text: &str) -> String {
        play(&mut Game::from_fen(fen).unwrap(), text)
    }

    #[test]
    fn game_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Game>();
    }

    #[test]
    fn san_basic_moves() {
        let mut game = Game::new();
        assert_eq!(play(&mut game, "e2e4"), "e4");
        assert_eq!(play(&mut game, "d7d5"), "d5");
        assert_eq!(play(&mut game, "e4d5"), "exd5");
        assert_eq!(play(&mut game, "g8f6"), "Nf6");
        assert_eq!(play(&mut game, "f1b5"), "Bb5+");
        let last = &game.moves()[4];
        assert_eq!((last.color, last.piece, last.captured), (Color::White, Piece::Bishop, None));
        assert_eq!(game.moves()[2].captured, Some(Piece::Pawn));
    }

    #[test]
    fn san_disambiguates_by_file() {
        assert_eq!(san_of("4k3/8/8/8/8/8/8/1N3NK1 w - - 0 1", "b1d2"), "Nbd2");
        assert_eq!(san_of("4k3/8/8/8/8/8/8/1N3NK1 w - - 0 1", "b1c3"), "Nc3");
    }

    #[test]
    fn san_disambiguates_by_rank() {
        assert_eq!(san_of("4k3/8/8/R7/8/8/8/R3K3 w - - 0 1", "a1a3"), "R1a3");
        assert_eq!(san_of("4k3/8/8/R7/8/8/8/R3K3 w - - 0 1", "a5a3"), "R5a3");
    }

    #[test]
    fn san_disambiguates_by_square() {
        assert_eq!(san_of("6k1/8/8/8/8/Q7/8/Q1Q4K w - - 0 1", "a1b2"), "Qa1b2");
    }

    #[test]
    fn san_castling() {
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        assert_eq!(san_of(fen, "e1g1"), "O-O");
        assert_eq!(san_of(fen, "e1c1"), "O-O-O");
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1";
        assert_eq!(san_of(fen, "e8g8"), "O-O");
        assert_eq!(san_of(fen, "e8c8"), "O-O-O");
    }

    #[test]
    fn san_promotion_with_capture_and_check() {
        let fen = "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1";
        assert_eq!(san_of(fen, "b7a8q"), "bxa8=Q+");
        assert_eq!(san_of(fen, "b7b8n"), "b8=N");
    }

    #[test]
    fn san_en_passant() {
        let mut game = Game::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        assert_eq!(play(&mut game, "e5d6"), "exd6");
        assert_eq!(game.moves()[0].captured, Some(Piece::Pawn));
        assert_eq!(game.piece_at(35), None);
    }

    #[test]
    fn fools_mate_is_checkmate() {
        let mut game = Game::new();
        for text in ["f2f3", "e7e5", "g2g4"] {
            play(&mut game, text);
        }
        assert_eq!(game.outcome(), None);
        assert_eq!(play(&mut game, "d8h4"), "Qh4#");
        assert!(game.in_check());
        assert_eq!(game.outcome(), Some(Outcome::Checkmate { winner: Color::Black }));
        let sans: Vec<&str> = game.moves().iter().map(|p| p.san.as_str()).collect();
        assert_eq!(sans, ["f3", "e5", "g4", "Qh4#"]);
        assert_eq!(game.history().len(), 4);
    }

    #[test]
    fn stalemate() {
        let mut game = Game::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        assert!(!game.in_check());
        assert_eq!(game.outcome(), Some(Outcome::Stalemate));
    }

    #[test]
    fn threefold_repetition() {
        let mut game = Game::new();
        for _ in 0..2 {
            assert_eq!(game.outcome(), None);
            for text in ["g1f3", "g8f6", "f3g1", "f6g8"] {
                play(&mut game, text);
            }
        }
        assert_eq!(game.outcome(), Some(Outcome::Repetition));
    }

    #[test]
    fn fifty_move_rule() {
        let mut game = Game::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 100 80").unwrap();
        assert_eq!(game.outcome(), Some(Outcome::FiftyMoves));
        let mut game = Game::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 99 80").unwrap();
        assert_eq!(game.outcome(), None);
    }

    #[test]
    fn insufficient_material() {
        let cases = [
            ("4k3/8/8/8/8/8/8/4K3 w - - 0 1", true),
            ("4k3/8/8/8/8/8/8/2B1K3 w - - 0 1", true),
            ("4k3/8/8/8/8/8/8/1N2K3 b - - 0 1", true),
            ("4kb2/8/8/8/8/8/8/2B1K3 w - - 0 1", true),
            ("2b1k3/8/8/8/8/8/8/2B1K3 w - - 0 1", false),
            ("4k3/8/8/8/8/8/8/1NB1K3 w - - 0 1", false),
            ("4k3/8/8/8/8/8/P7/4K3 w - - 0 1", false),
        ];
        for (fen, expected) in cases {
            let outcome = Game::from_fen(fen).unwrap().outcome();
            assert_eq!(outcome == Some(Outcome::InsufficientMaterial), expected, "{fen}");
        }
    }

    #[test]
    fn rejects_illegal_moves_and_tracks_state() {
        let mut game = Game::new();
        let illegal = crate::r#move::Move::new(12, 36, 0, 0);
        assert!(game.play(illegal).is_err());
        assert!(game.moves().is_empty());
        assert_eq!(game.side_to_move(), Color::White);
        assert_eq!(game.piece_at(4), Some((Color::White, Piece::King)));
        assert_eq!(game.king_square(Color::Black), 60);
        let start_key = game.board().zobrist_key;
        play(&mut game, "e2e4");
        assert_eq!(game.side_to_move(), Color::Black);
        assert_eq!(game.history(), [start_key]);
    }
}
