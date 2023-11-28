use aoc_client::{
    LeaderboardId, PuzzleDay, PuzzleYear, DEFAULT_PUZZLE_DESCRIPTION,
    DEFAULT_PUZZLE_INPUT,
};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, infer_subcommands = true)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Puzzle day [default: last unlocked day (during Advent of Code month)]
    #[arg(short, long, global = true)]
    pub day: Option<PuzzleDay>,

    /// Puzzle year [default: year of current or last Advent of Code event]
    #[arg(short, long, global = true)]
    pub year: Option<PuzzleYear>,

    /// Path to session cookie file [default: ~/.adventofcode.session]
    #[arg(short, long, alias = "session", global = true, value_name = "PATH")]
    pub session_file: Option<String>,

    /// Width at which to wrap output [default: terminal width]
    #[arg(short, long, global = true)]
    pub width: Option<usize>,

    /// Overwrite files if they already exist
    #[arg(short, long, global = true)]
    pub overwrite: bool,

    /// Download puzzle input only
    #[arg(short = 'I', long, global = true)]
    pub input_only: bool,

    /// Download puzzle description only
    #[arg(
        short = 'P',
        short_alias = 'D',
        long,
        alias = "description-only",
        global = true,
        conflicts_with = "input_only"
    )]
    pub puzzle_only: bool,

    /// Path where to save puzzle input
    #[arg(
        short,
        long,
        alias = "input",
        global = true,
        value_name = "PATH",
        default_value = DEFAULT_PUZZLE_INPUT,
    )]
    pub input_file: String,

    /// Path where to save puzzle description
    #[arg(
        short,
        long,
        alias = "puzzle",
        global = true,
        value_name = "PATH",
        default_value = DEFAULT_PUZZLE_DESCRIPTION,
    )]
    pub puzzle_file: String,

    /// Show HTML markup including links
    #[arg(short = 'm', long, global = true)]
    pub show_html_markup: bool,

    /// Restrict log messages to errors only
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Enable debug logging
    #[arg(long, global = true, conflicts_with = "quiet")]
    pub debug: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Show Advent of Code calendar and stars collected
    #[command(visible_alias = "c")]
    Calendar,

    /// Save puzzle description and input to files
    #[command(visible_alias = "d")]
    Download,

    /// Read puzzle statement (the default command)
    #[command(visible_alias = "r")]
    Read,

    /// Create a config file
    #[command(visible_alias = "i")]
    Init,

    /// Set a value in the config
    #[command(visible_alias = "se")]
    SetConfig(SetConfig),

    /// Submit puzzle answer
    #[command(visible_alias = "s")]
    Submit {
        /// Puzzle part
        #[arg(value_parser = ["1", "2"])]
        part: String,

        /// Puzzle answer
        answer: String,
    },

    /// Show the state of a private leaderboard
    #[command(visible_alias = "p")]
    PrivateLeaderboard {
        /// Private leaderboard ID
        leaderboard_id: Option<LeaderboardId>,
    },
}

#[derive(Parser, Debug)]
#[command(version, about, infer_subcommands = true)]
pub struct SetConfig {
    /// Set the config puzzle year
    #[arg(visible_alias = "y", long)]
    pub year: Option<PuzzleYear>,

    /// Set the config puzzle day
    #[arg(visible_alias = "d", long)]
    pub day: Option<PuzzleDay>,

    /// Set the config session filename
    #[arg(visible_alias = "sf", long)]
    pub session_file: Option<String>,

    /// Set the width
    #[arg(visible_alias = "w", long)]
    pub width: Option<usize>,

    /// Set the config input filename
    #[arg(visible_alias = "if", long)]
    pub input_filename: Option<String>,

    /// Set the config description filename
    #[arg(visible_alias = "df", long)]
    pub description_filename: Option<String>,

    /// Set the config private leaderboard id
    #[arg(visible_alias = "id", long)]
    pub private_leaderboard_id: Option<LeaderboardId>,
}
