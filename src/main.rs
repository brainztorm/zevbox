use std::process::ExitCode;

fn main() -> ExitCode {
    let code = zevbox::cli::run();
    std::process::exit(code as i32);
}
