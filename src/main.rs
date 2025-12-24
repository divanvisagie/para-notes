use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod serve;

#[derive(Parser)]
#[command(name = "para", version, about = "PARA notes web server")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Serve Notes directory as a web interface
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "8989")]
        port: u16,
        /// Override Notes root directory
        #[arg(long)]
        notes_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { port, notes_dir } => {
            let root = resolve_notes_dir(notes_dir)?;
            serve::run_server(root, port).await?;
        }
    }

    Ok(())
}

fn resolve_notes_dir(notes_dir: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = notes_dir {
        return Ok(path);
    }

    let home = std::env::var("HOME")?;
    Ok(PathBuf::from(home).join("src").join("Notes"))
}
