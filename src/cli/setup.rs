use crate::error::chronicle_error::SetupSnafu;
use crate::error::Result;
use crate::setup::{SetupOptions, SetupReport};
use snafu::ResultExt;

pub fn run(
    force: bool,
    dry_run: bool,
    skip_skills: bool,
    skip_hooks: bool,
    skip_claude_md: bool,
) -> Result<()> {
    let options = SetupOptions {
        force,
        dry_run,
        skip_skills,
        skip_hooks,
        skip_claude_md,
    };

    let report = crate::setup::run_setup(&options).context(SetupSnafu)?;
    print_report(&report, dry_run);
    Ok(())
}

fn print_report(report: &SetupReport, dry_run: bool) {
    if dry_run {
        return;
    }
    eprintln!();
    eprintln!("Chronicle setup complete!");
    eprintln!();

    if !report.skills_installed.is_empty() {
        eprintln!(
            "  Skills:      {}",
            report.skills_installed[0]
                .parent()
                .expect("skill path always has parent")
                .display()
        );
    }

    for hook in &report.hooks_installed {
        eprintln!("  Hook:        {}", hook.display());
    }

    if report.claude_md_updated {
        eprintln!("  CLAUDE.md:   updated (Chronicle section added)");
    }

    eprintln!();
    eprintln!("Next: cd your-project && git chronicle init");
}
