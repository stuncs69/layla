use std::io::{self, BufRead, Write};
use std::time::Duration;

use crate::board::{Board, STANDARD_STARTPOS_FEN};
use crate::perft::perft;
use crate::search::{SearchLimits, Searcher};

pub fn run() -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut searcher = Searcher::new();
    let mut board = Board::startpos(&searcher.zobrist);

    for line in stdin.lock().lines() {
        let line = line.map_err(|err| err.to_string())?;
        let command = line.trim();
        if command.is_empty() {
            continue;
        }

        if command == "uci" {
            writeln!(stdout, "id name Layla").map_err(|err| err.to_string())?;
            writeln!(stdout, "id author stuncs69").map_err(|err| err.to_string())?;
            writeln!(stdout, "uciok").map_err(|err| err.to_string())?;
        } else if command == "isready" {
            writeln!(stdout, "readyok").map_err(|err| err.to_string())?;
        } else if command == "ucinewgame" {
            board = Board::startpos(&searcher.zobrist);
            searcher.clear();
        } else if command.starts_with("position ") {
            parse_position(command, &mut board, &mut searcher)?;
        } else if command.starts_with("go") {
            let limits = parse_go(command);
            let result = searcher.search(&mut board, limits);
            writeln!(stdout, "bestmove {}", result.best_move.to_uci())
                .map_err(|err| err.to_string())?;
        } else if command.starts_with("perft ") {
            let depth = command[6..]
                .trim()
                .parse::<usize>()
                .map_err(|err| err.to_string())?;
            let nodes = perft(&mut board, depth, &searcher.attacks, &searcher.zobrist);
            writeln!(stdout, "nodes {nodes}").map_err(|err| err.to_string())?;
        } else if command == "quit" {
            break;
        }

        stdout.flush().map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn parse_position(command: &str, board: &mut Board, searcher: &mut Searcher) -> Result<(), String> {
    let rest = command
        .strip_prefix("position ")
        .ok_or_else(|| String::from("bad position"))?;
    let mut tokens = rest.split_whitespace();
    let first = tokens
        .next()
        .ok_or_else(|| String::from("missing position args"))?;

    if first == "startpos" {
        *board = Board::from_fen(STANDARD_STARTPOS_FEN, &searcher.zobrist)?;
    } else if first == "fen" {
        let fen_parts: Vec<&str> = tokens.by_ref().take(6).collect();
        if fen_parts.len() != 6 {
            return Err(String::from("incomplete FEN"));
        }
        *board = Board::from_fen(&fen_parts.join(" "), &searcher.zobrist)?;
    } else {
        return Err(String::from("unsupported position command"));
    }

    let trailing: Vec<&str> = tokens.collect();
    if let Some(idx) = trailing.iter().position(|tok| *tok == "moves") {
        for mv_text in &trailing[idx + 1..] {
            let mv = board
                .parse_uci_move(mv_text, &searcher.attacks, &searcher.zobrist)
                .ok_or_else(|| format!("illegal move: {mv_text}"))?;
            let _undo = board
                .make_move(mv, &searcher.attacks, &searcher.zobrist)
                .ok_or_else(|| format!("failed to make move: {mv_text}"))?;
        }
    }

    Ok(())
}

fn parse_go(command: &str) -> SearchLimits {
    let mut limits = SearchLimits::default();
    let mut tokens = command.split_whitespace().skip(1);

    while let Some(token) = tokens.next() {
        match token {
            "depth" => limits.depth = tokens.next().and_then(|v| v.parse::<usize>().ok()),
            "movetime" => {
                limits.movetime = tokens
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(Duration::from_millis);
            }
            "wtime" => limits.wtime = tokens.next().and_then(|v| v.parse::<u64>().ok()),
            "btime" => limits.btime = tokens.next().and_then(|v| v.parse::<u64>().ok()),
            "winc" => limits.winc = tokens.next().and_then(|v| v.parse::<u64>().ok()),
            "binc" => limits.binc = tokens.next().and_then(|v| v.parse::<u64>().ok()),
            "movestogo" => limits.movestogo = tokens.next().and_then(|v| v.parse::<u64>().ok()),
            _ => {}
        }
    }

    if limits.depth.is_none()
        && limits.movetime.is_none()
        && limits.wtime.is_none()
        && limits.btime.is_none()
    {
        limits.depth = Some(6);
    }

    limits
}
