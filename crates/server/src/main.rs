use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(name = "xszs", version, about = "Media library service")]
struct Cli {
    #[arg(long)]
    root: PathBuf,
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let path = cli.root.join("config/config.json");
    let result = match xszs::config::Config::from_root_sources(&cli.root, &path) {
        Ok(config) => xszs::serve(config).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xszs could not start: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
