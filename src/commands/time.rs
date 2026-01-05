use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum TimeCommands {
    /// Log time spent on an issue
    Log {
        /// Issue ID
        issue: String,
        /// Time spent (e.g., "2h", "30m")
        duration: String,
    },
    /// List time entries
    List {
        /// Filter by issue ID
        #[arg(short, long)]
        issue: Option<String>,
    },
}

pub async fn handle(cmd: TimeCommands) -> Result<()> {
    match cmd {
        TimeCommands::Log { issue, duration } => {
            println!("Time tracking not yet implemented.");
            println!("Would log {} on issue {}", duration, issue);
            Ok(())
        }
        TimeCommands::List { issue } => {
            println!("Time tracking not yet implemented.");
            if let Some(id) = issue {
                println!("Would list time entries for issue {}", id);
            } else {
                println!("Would list all time entries");
            }
            Ok(())
        }
    }
}
