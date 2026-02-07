use crate::error::Result;
use crate::git::CliOps;

pub fn run(path: String, anchor: Option<String>, commit: String, no_tui: bool) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let data = crate::show::build_show_data(&git_ops, &path, &commit, anchor.as_deref())?;

    let use_tui = !no_tui && std::io::IsTerminal::is_terminal(&std::io::stdout());

    if use_tui {
        #[cfg(feature = "tui")]
        {
            return crate::show::run_tui(data);
        }
        #[cfg(not(feature = "tui"))]
        {
            return run_plain_output(&data);
        }
    }

    run_plain_output(&data)
}

fn run_plain_output(data: &crate::show::ShowData) -> Result<()> {
    crate::show::run_plain(data, &mut std::io::stdout()).map_err(|e| {
        crate::error::ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        }
    })
}
