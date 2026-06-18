fn main() {
    if let Err(error) = codebase::run_cli() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
