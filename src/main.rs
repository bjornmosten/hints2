use clap::Parser;

/// Simple placeholder for the hints CLI.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Subcommand to run
    #[arg(short, long)]
    command: Option<String>,
}

fn main() {
    let args = Args::parse();
    match args.command.as_deref() {
        Some("run") => {
            // TODO: implement main hint logic
            println!("Running hints service...");
        }
        Some(other) => {
            println!("Unknown command: {}", other);
        }
        None => {
            println!("No command provided. Use --help for usage.");
        }
    }
}
