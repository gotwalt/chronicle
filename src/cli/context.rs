use crate::cli::ContextAction;
use crate::error::Result;
use crate::hooks::{
    delete_pending_context, read_pending_context, write_pending_context, PendingContext,
};

use super::util::find_git_dir;

pub fn run(action: ContextAction) -> Result<()> {
    let git_dir = find_git_dir()?;

    match action {
        ContextAction::Set {
            task,
            reasoning,
            dependencies,
            tags,
        } => {
            let ctx = PendingContext {
                task,
                reasoning,
                dependencies,
                tags,
            };
            write_pending_context(&git_dir, &ctx)?;
            eprintln!("pending context saved");
        }
        ContextAction::Show => match read_pending_context(&git_dir)? {
            Some(ctx) => {
                println!("{}", serde_json::to_string_pretty(&ctx).unwrap_or_default());
            }
            None => {
                eprintln!("no pending context");
            }
        },
        ContextAction::Clear => {
            delete_pending_context(&git_dir)?;
            eprintln!("pending context cleared");
        }
    }

    Ok(())
}

