//! VSA CLI - Vertical Slice Architecture Manager
//!
//! Command-line tool for managing vertical slice architecture in event-sourced systems.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;
use tracing_subscriber::EnvFilter;

mod commands;
mod templates;

use commands::{generate, init, list, manifest, validate};

/// VSA - Vertical Slice Architecture Manager
#[derive(Parser)]
#[command(name = "vsa")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "vsa.yaml", global = true)]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize VSA configuration
    Init {
        /// Root directory for contexts
        #[arg(short, long, default_value = "./src/contexts")]
        root: PathBuf,

        /// Primary language (typescript, python, rust)
        #[arg(short, long, default_value = "typescript")]
        language: String,

        /// Enable framework integration
        #[arg(long)]
        with_framework: bool,
    },

    /// Validate VSA structure
    Validate {
        /// Fix auto-fixable issues
        #[arg(long)]
        fix: bool,

        /// Watch for changes
        #[arg(short, long)]
        watch: bool,
    },

    /// Generate new feature
    Generate {
        /// Context name
        #[arg(short, long)]
        context: String,

        /// Feature name
        #[arg(short, long)]
        feature: String,

        /// Feature type (command, query, event)
        #[arg(short = 't', long)]
        feature_type: Option<String>,

        /// Interactive mode
        #[arg(short, long)]
        interactive: bool,
    },

    /// List contexts and features
    List {
        /// Show only contexts
        #[arg(long)]
        contexts_only: bool,

        /// Filter by context
        #[arg(short, long)]
        context: Option<String>,

        /// Output format (text, json, tree)
        #[arg(short = 'f', long, default_value = "tree")]
        format: String,
    },

    /// Generate manifest
    Manifest {
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format (json, yaml)
        #[arg(short, long, default_value = "json")]
        format: String,
    },
}

fn main() {
    let cli = Cli::parse();

    // Setup logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(EnvFilter::new(format!("vsa={log_level}"))).init();

    let result = match cli.command {
        Commands::Init { root, language, with_framework } => {
            init::run(root, language, with_framework)
        }

        Commands::Validate { fix, watch } => validate::run(&cli.config, fix, watch),

        Commands::Generate { context, feature, feature_type, interactive } => {
            generate::run(&cli.config, context, feature, feature_type, interactive)
        }

        Commands::List { contexts_only, context, format } => {
            list::run(&cli.config, contexts_only, context, format)
        }

        Commands::Manifest { output, format } => manifest::run(&cli.config, output, format),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
