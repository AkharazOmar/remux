mod app;
mod video;
mod com;

use app::App;

use tokio::signal;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create and run the application
    let mut app = App::new().await?;

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
