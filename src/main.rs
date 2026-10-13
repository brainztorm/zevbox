use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(zevbox::cli::run())
}
