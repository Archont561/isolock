//! Thin entrypoint: the composition root in `cli` owns every construction.

fn main() -> std::process::ExitCode {
    isolock::run()
}
