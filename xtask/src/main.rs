use xshell::{Cmd, Shell};

// TODO: nice error formatting
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sh = &Shell::new()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(|s| &**s).collect();
    match *args {
        ["test"] => {
            cargo(sh).args(["test", "--all-features"]).run()?;

            // Run tests with fewer features quietly to reduce spam
            cargo(sh)
                .args(["--quiet", "test", "--no-default-features"])
                .run()?;
            for additional_feature in ["async", "std", "spin-sync", "std-sync"] {
                cargo(sh)
                    .args(["--quiet", "test", "--no-default-features"])
                    .arg(format!("--features={additional_feature}"))
                    .run()?;
            }

            cargo(sh)
                .args(["clippy", "--all-features", "--all-targets"])
                .run()?;
            cargo(sh).args(["doc", "--all-features"]).run()?;
        }
        _ => {
            return Err(format!("invalid arguments: {args:?}").into());
        }
    }

    Ok(())
}

fn cargo(sh: &Shell) -> Cmd<'_> {
    sh.cmd(std::env::var("CARGO").expect("CARGO environment variable not set"))
}
