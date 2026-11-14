# Layla

Layla is a UCI chess engine written in Rust without any dependencies.

Build and run:

```bash
cargo run --release
```

Example UCI session:

```text
uci
isready
position startpos
go depth 5
quit
```

You can also load `target/release/layla` into any UCI-compatible chess GUI.

## Use as a library

Add Layla as a path or git dependency (`layla = { path = "../layla" }`), then
drive a game with `Game` and ask `Searcher` for moves:

```rust
use std::sync::atomic::AtomicBool;

use layla::game::Game;
use layla::search::{SearchLimits, Searcher};

let mut game = Game::new();
let mut searcher = Searcher::new(64);
let stop = AtomicBool::new(false);

while game.outcome().is_none() {
    let mut board = game.board().clone();
    let limits = SearchLimits { depth: Some(6), ..SearchLimits::default() };
    let result = searcher.search(&mut board, game.history(), limits, &stop, &mut |it| {
        println!("depth {} score {:?} pv {:?}", it.depth, it.score, it.pv);
    });
    let played = game.play(result.best_move).expect("engine move is legal");
    println!("{}", played.san);
}
println!("{:?}", game.outcome());
```

`Game` tracks SAN, captured pieces, position history and outcomes (checkmate,
stalemate, threefold repetition, fifty-move rule, insufficient material).
`Searcher::search` reports each completed depth through the callback and always
returns a legal move when one exists, even when stopped early.
