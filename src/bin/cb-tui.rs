fn main() {
    if let Err(error) = codebase::run_cb_tui() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
