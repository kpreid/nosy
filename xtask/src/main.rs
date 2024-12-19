use xshell::{Cmd, Shell};

// TODO: nice error formatting
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sh = &Shell::new()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(|s| &**s).collect();
    match *args {
        ["test"] => {
            cargo(sh).args(["test", "--no-default-features"]).run()?;
            cargo(sh).args(["test", "--features=async"]).run()?;
            cargo(sh).args(["test", "--features=std"]).run()?;
            cargo(sh).args(["test", "--features=sync"]).run()?;
            cargo(sh).args(["test", "--all-features"]).run()?;
            cargo(sh).args(["clippy", "--all-features"]).run()?;
            cargo(sh).args(["doc", "--all-features"]).run()?;
        }
        _ => {
            return Err(format!("invalid arguments: {args:?}").into());
        }
    }

    Ok(())
}

fn cargo(sh: &Shell) -> Cmd {
    sh.cmd(std::env::var("CARGO").expect("CARGO environment variable not set"))
}
