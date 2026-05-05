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
