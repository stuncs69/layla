use crate::board::Board;
use crate::movegen::generate_legal_moves;
use crate::zobrist::{AttackTables, ZobristTables};

pub fn perft(board: &mut Board, depth: usize, attacks: &AttackTables, zobrist: &ZobristTables) -> u64 {
    if depth == 0 {
        return 1;
    }

    let moves = generate_legal_moves(board, attacks, zobrist, false);
    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0u64;
    for mv in moves.as_slice() {
        let undo = board
            .make_move(*mv, attacks, zobrist)
            .expect("generated move should be legal");
        nodes += perft(board, depth - 1, attacks, zobrist);
        board.unmake_move(*mv, undo, zobrist);
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::perft;
    use crate::board::Board;
    use crate::zobrist::{AttackTables, ZobristTables};

    #[test]
    fn start_position_perft() {
        let attacks = AttackTables::new();
        let zobrist = ZobristTables::new();
        let mut board = Board::startpos(&zobrist);
        assert_eq!(perft(&mut board, 1, &attacks, &zobrist), 20);
        assert_eq!(perft(&mut board, 2, &attacks, &zobrist), 400);
        assert_eq!(perft(&mut board, 3, &attacks, &zobrist), 8902);
        assert_eq!(perft(&mut board, 4, &attacks, &zobrist), 197281);
    }

    #[test]
    fn kiwipete_perft_depth_3() {
        let attacks = AttackTables::new();
        let zobrist = ZobristTables::new();
        let mut board = Board::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            &zobrist,
        )
        .unwrap();
        assert_eq!(perft(&mut board, 1, &attacks, &zobrist), 48);
        assert_eq!(perft(&mut board, 2, &attacks, &zobrist), 2039);
        assert_eq!(perft(&mut board, 3, &attacks, &zobrist), 97862);
    }
}
