use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::board::{decode_piece, Board, Color, Piece, Undo};
use crate::eval::{evaluate, piece_value};
use crate::movegen::generate_legal_moves;
use crate::r#move::Move;
use crate::tt::{TranspositionTable, TT_ALPHA, TT_BETA, TT_EXACT};
use crate::zobrist::{AttackTables, ZobristTables};

const INF: i32 = 30_000;
const MATE: i32 = 29_000;
const MATE_BOUND: i32 = 28_000;
const MAX_PLY: usize = 128;
const MAX_PV: usize = 32;

#[derive(Default, Clone, Copy)]
pub struct SearchLimits {
    pub depth: Option<usize>,
    pub movetime: Option<Duration>,
    pub wtime: Option<u64>,
    pub btime: Option<u64>,
    pub winc: Option<u64>,
    pub binc: Option<u64>,
    pub movestogo: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    Cp(i32),
    Mate(i32),
}

impl Score {
    fn from_search(score: i32) -> Self {
        if score.abs() >= MATE_BOUND {
            if score > 0 {
                Self::Mate((MATE - score + 1) / 2)
            } else {
                Self::Mate(-(MATE + score + 1) / 2)
            }
        } else {
            Self::Cp(score)
        }
    }
}

#[derive(Clone, Debug)]
pub struct Iteration {
    pub depth: usize,
    pub score: Score,
    pub nodes: u64,
    pub nps: u64,
    pub time_ms: u64,
    pub pv: Vec<Move>,
}

pub struct SearchResult {
    pub best_move: Move,
}

pub struct Searcher {
    pub tt: TranspositionTable,
    pub attacks: AttackTables,
    pub zobrist: ZobristTables,
    killer_moves: [[Move; 2]; MAX_PLY],
    history: [[[i32; 64]; 64]; 2],
    keys: Vec<u64>,
    repetition_floor: usize,
    root_best: Move,
    nodes: u64,
    start_time: Instant,
    stop_time: Option<Instant>,
    stopped: bool,
}

impl Searcher {
    pub fn new(tt_mb: usize) -> Self {
        Self {
            tt: TranspositionTable::new(tt_mb),
            attacks: AttackTables::new(),
            zobrist: ZobristTables::new(),
            killer_moves: [[Move::NULL; 2]; MAX_PLY],
            history: [[[0; 64]; 64]; 2],
            keys: Vec::new(),
            repetition_floor: 0,
            root_best: Move::NULL,
            nodes: 0,
            start_time: Instant::now(),
            stop_time: None,
            stopped: false,
        }
    }

    pub fn clear(&mut self) {
        self.tt.clear();
        self.killer_moves = [[Move::NULL; 2]; MAX_PLY];
        self.history = [[[0; 64]; 64]; 2];
    }

    pub fn search(
        &mut self,
        board: &mut Board,
        history: &[u64],
        limits: SearchLimits,
        stop: &AtomicBool,
        on_iteration: &mut dyn FnMut(&Iteration),
    ) -> SearchResult {
        self.nodes = 0;
        self.stopped = false;
        self.start_time = Instant::now();
        self.stop_time = compute_stop_time(board.side_to_move, limits, self.start_time);
        self.keys.clear();
        self.keys.extend_from_slice(history);
        self.repetition_floor = 0;

        let legal = generate_legal_moves(board, &self.attacks, &self.zobrist, false);
        let Some(&first_legal) = legal.as_slice().first() else {
            return SearchResult {
                best_move: Move::NULL,
            };
        };
        let mut best_move = first_legal;

        let max_depth = limits.depth.unwrap_or(64);
        for depth in 1..=max_depth {
            self.root_best = Move::NULL;
            let score = self.negamax(board, depth as i32, 0, -INF, INF, stop);
            if self.stopped {
                break;
            }
            if !self.root_best.is_null() {
                best_move = self.root_best;
            }

            let pv = self.extract_pv(board, best_move, depth);
            let time_ms = self.start_time.elapsed().as_millis().max(1) as u64;
            on_iteration(&Iteration {
                depth,
                score: Score::from_search(score),
                nodes: self.nodes,
                nps: self.nodes.saturating_mul(1000) / time_ms,
                time_ms,
                pv,
            });
        }

        SearchResult { best_move }
    }

    fn extract_pv(&self, board: &mut Board, first: Move, depth: usize) -> Vec<Move> {
        let max_len = depth.clamp(1, MAX_PV);
        let mut pv = Vec::with_capacity(max_len);
        let mut undos: Vec<Undo> = Vec::with_capacity(max_len);
        let mut seen = Vec::with_capacity(max_len + 1);
        seen.push(board.zobrist_key);

        let mut mv = first;
        while !mv.is_null() && pv.len() < max_len {
            let legal = generate_legal_moves(board, &self.attacks, &self.zobrist, false);
            if !legal.as_slice().contains(&mv) {
                break;
            }
            let undo = board
                .make_move(mv, &self.attacks, &self.zobrist)
                .expect("legal move remained legal");
            pv.push(mv);
            undos.push(undo);
            if seen.contains(&board.zobrist_key) {
                break;
            }
            seen.push(board.zobrist_key);
            mv = self
                .tt
                .probe(board.zobrist_key)
                .map_or(Move::NULL, |entry| entry.best_move);
        }

        for (mv, undo) in pv.iter().zip(undos).rev() {
            board.unmake_move(*mv, undo, &self.zobrist);
        }
        pv
    }

    fn is_repetition(&self, board: &Board) -> bool {
        let len = self.keys.len();
        let window = (board.halfmove_clock as usize).min(len - self.repetition_floor);
        (2..=window)
            .step_by(2)
            .any(|back| self.keys[len - back] == board.zobrist_key)
    }

    fn negamax(
        &mut self,
        board: &mut Board,
        depth: i32,
        ply: usize,
        mut alpha: i32,
        beta: i32,
        stop: &AtomicBool,
    ) -> i32 {
        if ply >= MAX_PLY - 1 {
            return evaluate(board, &self.attacks);
        }
        if self.should_stop(stop) {
            self.stopped = true;
            return 0;
        }
        if ply > 0 && (board.halfmove_clock >= 100 || self.is_repetition(board)) {
            return 0;
        }
        self.nodes += 1;

        let alpha_orig = alpha;
        let in_check = board.in_check(board.side_to_move, &self.attacks);

        if depth <= 0 {
            return self.quiescence(board, ply, alpha, beta, stop);
        }

        if ply > 0 {
            if let Some(entry) = self.tt.probe(board.zobrist_key) {
                if entry.depth as i32 >= depth {
                    let score = tt_score_to_search(entry.score, ply);
                    match entry.flag {
                        TT_EXACT => return score,
                        TT_ALPHA if score <= alpha => return score,
                        TT_BETA if score >= beta => return score,
                        _ => {}
                    }
                }
            }
        }

        let static_eval = evaluate(board, &self.attacks);
        if depth >= 3
            && !in_check
            && static_eval >= beta
            && has_non_pawn_material(board, board.side_to_move)
        {
            let saved_ep = board.en_passant;
            let saved_halfmove = board.halfmove_clock;
            let saved_fullmove = board.fullmove_number;
            let saved_key = board.zobrist_key;
            let saved_side = board.side_to_move;
            let saved_floor = self.repetition_floor;

            self.keys.push(saved_key);
            self.repetition_floor = self.keys.len();
            if board.en_passant != crate::board::NO_SQ {
                board.zobrist_key ^= self.zobrist.en_passant[(board.en_passant & 7) as usize];
                board.en_passant = crate::board::NO_SQ;
            }
            if saved_side == Color::Black {
                board.fullmove_number += 1;
            }
            board.halfmove_clock += 1;
            board.side_to_move = board.side_to_move.opposite();
            board.zobrist_key ^= self.zobrist.side;

            let reduction = (2 + depth / 4).min(4);
            let score =
                -self.negamax(board, depth - 1 - reduction, ply + 1, -beta, -beta + 1, stop);

            board.side_to_move = saved_side;
            board.en_passant = saved_ep;
            board.halfmove_clock = saved_halfmove;
            board.fullmove_number = saved_fullmove;
            board.zobrist_key = saved_key;
            self.repetition_floor = saved_floor;
            self.keys.pop();

            if self.stopped {
                return 0;
            }
            if score >= beta {
                return beta;
            }
        }

        let mut moves = generate_legal_moves(board, &self.attacks, &self.zobrist, false);
        if moves.is_empty() {
            if in_check {
                return -MATE + ply as i32;
            }
            return 0;
        }

        let hash_move = self.tt.probe(board.zobrist_key).map(|e| e.best_move).unwrap_or(Move::NULL);
        let mut scores = [0i32; 256];
        for (idx, mv) in moves.as_slice().iter().enumerate() {
            scores[idx] = self.score_move(board, *mv, hash_move, ply);
        }

        let mut best_move = Move::NULL;
        let mut best_score = -INF;

        for i in 0..moves.len() {
            let mv = select_best(moves.as_mut_slice(), &mut scores, i);
            self.keys.push(board.zobrist_key);
            let undo = board
                .make_move(mv, &self.attacks, &self.zobrist)
                .expect("legal move remained legal");
            let gives_check = board.in_check(board.side_to_move, &self.attacks);
            let mut score;

            let can_reduce = depth >= 3
                && i >= 3
                && !in_check
                && !gives_check
                && !mv.is_capture()
                && !mv.is_promotion();
            if can_reduce {
                let reduction = lmr_reduction(depth, i as i32);
                let reduced_depth = (depth - 1 - reduction).max(0);
                score = -self.negamax(board, reduced_depth, ply + 1, -alpha - 1, -alpha, stop);

                if score > alpha {
                    score = -self.negamax(board, depth - 1, ply + 1, -alpha - 1, -alpha, stop);
                    if score > alpha && score < beta {
                        score = -self.negamax(board, depth - 1, ply + 1, -beta, -alpha, stop);
                    }
                }
            } else {
                if i == 0 {
                    score = -self.negamax(board, depth - 1, ply + 1, -beta, -alpha, stop);
                } else {
                    score = -self.negamax(board, depth - 1, ply + 1, -alpha - 1, -alpha, stop);
                    if score > alpha && score < beta {
                        score = -self.negamax(board, depth - 1, ply + 1, -beta, -alpha, stop);
                    }
                }
            }
            board.unmake_move(mv, undo, &self.zobrist);
            self.keys.pop();

            if self.stopped {
                return 0;
            }

            if score > best_score {
                best_score = score;
                best_move = mv;
            }
            if score > alpha {
                alpha = score;
                if !mv.is_capture() {
                    self.history[board.side_to_move.idx()][mv.from() as usize][mv.to() as usize] += depth * depth;
                }
            }
            if alpha >= beta {
                if !mv.is_capture() {
                    self.store_killer(ply, mv);
                }
                self.tt.store(
                    board.zobrist_key,
                    depth,
                    search_score_to_tt(beta, ply),
                    TT_BETA,
                    mv,
                );
                return beta;
            }
        }

        if ply == 0 {
            self.root_best = best_move;
        }
        let flag = if best_score <= alpha_orig { TT_ALPHA } else { TT_EXACT };
        self.tt.store(
            board.zobrist_key,
            depth,
            search_score_to_tt(best_score, ply),
            flag,
            best_move,
        );
        best_score
    }

    fn quiescence(
        &mut self,
        board: &mut Board,
        ply: usize,
        mut alpha: i32,
        beta: i32,
        stop: &AtomicBool,
    ) -> i32 {
        if ply >= MAX_PLY - 1 {
            return evaluate(board, &self.attacks);
        }
        if self.should_stop(stop) {
            self.stopped = true;
            return 0;
        }
        if board.halfmove_clock >= 100 {
            return 0;
        }
        self.nodes += 1;

        let stand_pat = evaluate(board, &self.attacks);
        if stand_pat >= beta {
            return beta;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }

        let mut moves = generate_legal_moves(board, &self.attacks, &self.zobrist, true);
        if moves.is_empty() {
            return stand_pat;
        }

        let hash_move = self.tt.probe(board.zobrist_key).map(|e| e.best_move).unwrap_or(Move::NULL);
        let mut scores = [0i32; 256];
        for (idx, mv) in moves.as_slice().iter().enumerate() {
            scores[idx] = self.score_move(board, *mv, hash_move, ply);
        }

        for i in 0..moves.len() {
            let mv = select_best(moves.as_mut_slice(), &mut scores, i);
            let undo = board
                .make_move(mv, &self.attacks, &self.zobrist)
                .expect("capture must be legal");
            let score = -self.quiescence(board, ply + 1, -beta, -alpha, stop);
            board.unmake_move(mv, undo, &self.zobrist);

            if self.stopped {
                return 0;
            }
            if score >= beta {
                return beta;
            }
            if score > alpha {
                alpha = score;
            }
        }
        alpha
    }

    fn score_move(&self, board: &Board, mv: Move, hash_move: Move, ply: usize) -> i32 {
        if mv == hash_move {
            return 1_000_000;
        }
        if mv.is_capture() {
            let attacker = decode_piece(board.piece_at(mv.from()))
                .map(|(_, p)| p)
                .unwrap_or(Piece::Pawn);
            let victim_square = if mv.is_en_passant() {
                if board.side_to_move == Color::White {
                    mv.to() - 8
                } else {
                    mv.to() + 8
                }
            } else {
                mv.to()
            };
            let victim = decode_piece(board.piece_at(victim_square))
                .map(|(_, p)| p)
                .unwrap_or(Piece::Pawn);
            return 100_000 + 16 * piece_value(victim) - piece_value(attacker);
        }
        if self.killer_moves[ply][0] == mv {
            return 90_000;
        }
        if self.killer_moves[ply][1] == mv {
            return 89_000;
        }
        self.history[board.side_to_move.idx()][mv.from() as usize][mv.to() as usize]
    }

    fn store_killer(&mut self, ply: usize, mv: Move) {
        if self.killer_moves[ply][0] != mv {
            self.killer_moves[ply][1] = self.killer_moves[ply][0];
            self.killer_moves[ply][0] = mv;
        }
    }

    fn should_stop(&self, stop: &AtomicBool) -> bool {
        stop.load(Ordering::Relaxed)
            || self.stop_time.is_some_and(|deadline| Instant::now() >= deadline)
    }
}

fn select_best(moves: &mut [Move], scores: &mut [i32; 256], start: usize) -> Move {
    let mut best_idx = start;
    for idx in start + 1..moves.len() {
        if scores[idx] > scores[best_idx] {
            best_idx = idx;
        }
    }
    moves.swap(start, best_idx);
    scores.swap(start, best_idx);
    moves[start]
}

fn compute_stop_time(side: Color, limits: SearchLimits, now: Instant) -> Option<Instant> {
    if let Some(movetime) = limits.movetime {
        return Some(now + movetime);
    }

    let (time, inc) = match side {
        Color::White => (limits.wtime, limits.winc),
        Color::Black => (limits.btime, limits.binc),
    };

    time.map(|remaining| {
        let moves = limits.movestogo.unwrap_or(30).max(1);
        let base = remaining / moves;
        let allowance = base + inc.unwrap_or(0) / 2;
        now + Duration::from_millis(allowance.max(1))
    })
}

fn search_score_to_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score + ply as i32
    } else if score <= -MATE_BOUND {
        score - ply as i32
    } else {
        score
    }
}

fn tt_score_to_search(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score - ply as i32
    } else if score <= -MATE_BOUND {
        score + ply as i32
    } else {
        score
    }
}

fn has_non_pawn_material(board: &Board, color: Color) -> bool {
    board.pieces[color.idx()][Piece::Knight as usize] != 0
        || board.pieces[color.idx()][Piece::Bishop as usize] != 0
        || board.pieces[color.idx()][Piece::Rook as usize] != 0
        || board.pieces[color.idx()][Piece::Queen as usize] != 0
}

fn lmr_reduction(depth: i32, move_index: i32) -> i32 {
    let mut reduction = 1;
    if depth >= 6 {
        reduction += 1;
    }
    if move_index >= 8 {
        reduction += 1;
    }
    reduction.min(depth - 2).max(1)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::{Iteration, Score, SearchLimits, Searcher};
    use crate::board::Board;
    use crate::game::Game;
    use crate::movegen::generate_legal_moves;
    use crate::r#move::Move;

    fn depth(depth: usize) -> SearchLimits {
        SearchLimits {
            depth: Some(depth),
            ..SearchLimits::default()
        }
    }

    fn run(
        searcher: &mut Searcher,
        board: &mut Board,
        history: &[u64],
        limits: SearchLimits,
    ) -> (Move, Vec<Iteration>) {
        let stop = AtomicBool::new(false);
        let mut iterations = Vec::new();
        let result = searcher.search(board, history, limits, &stop, &mut |it| {
            iterations.push(it.clone())
        });
        (result.best_move, iterations)
    }

    #[test]
    fn searcher_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Searcher>();
        assert_send::<Board>();
    }

    #[test]
    fn preset_stop_returns_legal_move() {
        let mut searcher = Searcher::new(1);
        let mut board = Board::startpos(&searcher.zobrist);
        let stop = AtomicBool::new(true);
        let mut calls = 0;
        let result = searcher.search(&mut board, &[], depth(5), &stop, &mut |_| calls += 1);
        assert_eq!(calls, 0);
        let legal = generate_legal_moves(&mut board, &searcher.attacks, &searcher.zobrist, false);
        assert!(legal.as_slice().contains(&result.best_move));
    }

    #[test]
    fn no_legal_moves_returns_null() {
        let mut searcher = Searcher::new(1);
        let mut board =
            Board::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", &searcher.zobrist).unwrap();
        let (best, iterations) = run(&mut searcher, &mut board, &[], depth(3));
        assert!(best.is_null());
        assert!(iterations.is_empty());
    }

    #[test]
    fn pv_starts_with_best_move_and_is_legal() {
        let mut searcher = Searcher::new(1);
        let mut board = Board::startpos(&searcher.zobrist);
        let key = board.zobrist_key;
        let (best, iterations) = run(&mut searcher, &mut board, &[], depth(5));
        assert_eq!(board.zobrist_key, key, "board restored after search");
        assert_eq!(iterations.len(), 5);
        assert_eq!(iterations.last().unwrap().pv[0], best);
        assert!(iterations.last().unwrap().pv.len() > 1);

        for it in &iterations {
            let mut undos = Vec::new();
            for mv in &it.pv {
                let legal =
                    generate_legal_moves(&mut board, &searcher.attacks, &searcher.zobrist, false);
                assert!(legal.as_slice().contains(mv), "illegal pv move {}", mv.to_uci());
                undos.push((*mv, board.make_move(*mv, &searcher.attacks, &searcher.zobrist).unwrap()));
            }
            for (mv, undo) in undos.into_iter().rev() {
                board.unmake_move(mv, undo, &searcher.zobrist);
            }
        }
    }

    #[test]
    fn finds_mate_in_one() {
        let mut searcher = Searcher::new(1);
        let mut board = Board::from_fen(
            "r1bqkbnr/pppp1ppp/2n5/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 0 1",
            &searcher.zobrist,
        )
        .unwrap();
        let (best, iterations) = run(&mut searcher, &mut board, &[], depth(3));
        assert_eq!(best.to_uci(), "h5f7");
        assert_eq!(iterations.last().unwrap().score, Score::Mate(1));
    }

    #[test]
    fn repetition_scores_as_draw() {
        let mut game = Game::from_fen("4k3/8/8/8/8/8/3q4/K5N1 w - - 0 1").unwrap();
        for text in ["g1f3", "e8e7", "f3g1", "e7e8"] {
            let mv = *game
                .legal_moves()
                .iter()
                .find(|mv| mv.to_uci() == text)
                .unwrap();
            game.play(mv).unwrap();
        }

        let mut searcher = Searcher::new(1);
        let mut board = game.board().clone();
        let (_, without_history) = run(&mut searcher, &mut board, &[], depth(4));
        assert!(matches!(without_history.last().unwrap().score, Score::Cp(cp) if cp < -300));

        searcher.clear();
        let (best, with_history) = run(&mut searcher, &mut board, game.history(), depth(4));
        assert_eq!(best.to_uci(), "g1f3");
        assert_eq!(with_history.last().unwrap().score, Score::Cp(0));
    }
}
