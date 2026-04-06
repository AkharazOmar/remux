mod app;
mod video;
mod com;
mod config;

use app::App;

use tokio::signal;

use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};


#[derive(Parser)]
#[command(name = "remux", version, about = "A video streaming application using Zenoh and Rust")]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long)]
    config: Option<String>,

    /// Generate shell completions
    #[arg(long, value_enum)]
    completions: Option<Shell>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Create and run the application
    if let Some(shell) = cli.completions {
        generate(shell, &mut Cli::command(), "remux", &mut std::io::stdout());
        return Ok(());
    }

    let mut app = App::new(cli.config.as_deref()).await?;

    // Run the app and wait for Ctrl+C
    tokio::select! {
        result = app.run() => {
            result?;
        }
        _ = signal::ctrl_c() => {
            println!("\nReceived Ctrl+C, shutting down...");
        }
    }

    Ok(())
}
