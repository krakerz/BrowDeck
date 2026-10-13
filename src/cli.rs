#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run,
    Help,
    Version,
    Error(String),
}

pub fn parse<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // No option takes a value and the app accepts no positional arguments,
    // so the first argument alone decides the outcome.
    let Some(arg) = args.into_iter().next() else {
        return Command::Run;
    };
    match arg.as_ref() {
        "-h" | "--help" => Command::Help,
        "-v" | "--version" => Command::Version,
        "--" => Command::Run,
        other if other.starts_with('-') => Command::Error(format!("unknown option '{other}'")),
        other => Command::Error(format!("unexpected argument '{other}'")),
    }
}

pub fn usage() -> String {
    format!(
        "BrowDeck {} — gamepad-first file browser\n\
         \n\
         Usage: browdeck [OPTIONS]\n\
         \n\
         Options:\n  \
           -h, --help       Print this help and exit\n  \
           -v, --version    Print the version and exit\n",
        env!("CARGO_PKG_VERSION")
    )
}

pub fn version() -> String {
    format!("BrowDeck {}", env!("CARGO_PKG_VERSION"))
}

/// Handles `--help`/`--version`/bad arguments; returns only when the GUI
/// should start.
pub fn handle_args() {
    match parse(std::env::args().skip(1)) {
        Command::Run => {}
        Command::Help => {
            print!("{}", usage());
            std::process::exit(0);
        }
        Command::Version => {
            println!("{}", version());
            std::process::exit(0);
        }
        Command::Error(msg) => {
            eprintln!("browdeck: {msg}\nTry 'browdeck --help' for more information.");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_runs() {
        assert_eq!(parse(Vec::<&str>::new()), Command::Run);
    }

    #[test]
    fn help_flags() {
        assert_eq!(parse(["-h"]), Command::Help);
        assert_eq!(parse(["--help"]), Command::Help);
    }

    #[test]
    fn version_flags() {
        assert_eq!(parse(["-v"]), Command::Version);
        assert_eq!(parse(["--version"]), Command::Version);
    }

    #[test]
    fn first_recognized_flag_wins() {
        assert_eq!(parse(["--version", "--help"]), Command::Version);
    }

    #[test]
    fn unknown_option_errors() {
        assert!(matches!(parse(["--bogus"]), Command::Error(_)));
        assert!(matches!(parse(["-x"]), Command::Error(_)));
    }

    #[test]
    fn positional_errors() {
        assert!(matches!(parse(["somefile"]), Command::Error(_)));
    }

    #[test]
    fn double_dash_ends_options() {
        assert_eq!(parse(["--"]), Command::Run);
        assert_eq!(parse(["--", "--help"]), Command::Run);
    }

    #[test]
    fn version_string_format() {
        assert_eq!(version(), format!("BrowDeck {}", env!("CARGO_PKG_VERSION")));
    }
}
