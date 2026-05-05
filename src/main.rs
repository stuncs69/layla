mod board;
mod eval;
mod movegen;
mod perft;
mod r#move;
mod search;
mod tt;
mod uci;
mod zobrist;

fn main() {
    if let Err(err) = uci::run() {
        eprintln!("{err}");
    }
}
