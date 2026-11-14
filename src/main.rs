fn main() {
    if let Err(err) = layla::uci::run() {
        eprintln!("{err}");
    }
}
