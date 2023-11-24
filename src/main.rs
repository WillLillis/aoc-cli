mod args;

use aoc_client::{
    AocClient, AocError, AocResult, ConfigPuzzleDay, ConfigPuzzleYear,
    DEFAULT_PUZZLE_DESCRIPTION, DEFAULT_PUZZLE_INPUT,
};
use args::{Args, Command};
use clap::{crate_description, crate_name, Parser};
use env_logger::{Builder, Env};
use exit_code::*;
use log::{error, info, warn, LevelFilter};
use std::process::exit;

fn main() {
    let args = Args::parse();

    setup_log(&args);

    info!("🎄 {} - {}", crate_name!(), crate_description!());

    match build_client(&args).and_then(|client| run(&args, client)) {
        Ok(_) => exit(SUCCESS),
        Err(err) => {
            error!("🔔 {err}");
            let exit_code = match err {
                AocError::InvalidPuzzleDate(..) => USAGE_ERROR,
                AocError::InvalidEventYear(..) => USAGE_ERROR,
                AocError::InvalidPuzzleDay(..) => USAGE_ERROR,
                AocError::LockedPuzzle(..) => USAGE_ERROR,
                AocError::SessionFileNotFound => NO_INPUT,
                AocError::SessionFileReadError { .. } => IO_ERROR,
                AocError::InvalidSessionCookie { .. } => DATA_ERROR,
                AocError::HttpRequestError { .. } => FAILURE,
                AocError::AocResponseError => FAILURE,
                AocError::PrivateLeaderboardNotAvailable => FAILURE,
                AocError::PrivateLeaderboardNoId => FAILURE,
                AocError::FileWriteError { .. } => CANNOT_CREATE,
                AocError::ConfigError(..) => DATA_ERROR,
                AocError::ClientFieldMissing(..) => USAGE_ERROR,
                AocError::InvalidPuzzlePart => USAGE_ERROR,
                AocError::InvalidOutputWidth => USAGE_ERROR,
            };

            if exit_code == FAILURE {
                // Unexpected responses from adventofcode.com including
                // HTTP 302/400/500 may be due to invalid or expired cookies
                warn!(
                    "🍪 Your session cookie may be invalid or expired, try \
                    logging in again"
                );
            }

            exit(exit_code);
        }
    };
}

fn setup_log(args: &Args) {
    let mut log_builder =
        Builder::from_env(Env::default().default_filter_or("info"));

    if args.quiet {
        log_builder.filter_module("aoc", LevelFilter::Error);
    } else if args.debug {
        log_builder.filter_module("aoc", LevelFilter::Debug);
    }

    log_builder.format_timestamp(None).init();
}

fn build_client(args: &Args) -> AocResult<AocClient> {
    let mut builder = AocClient::builder();
    let config = AocClient::get_config();

    match (&args.session_file, &config.session_file) {
        (Some(ref file), _) | (_, Some(ref file)) => {
            builder.session_cookie_from_file(file)?;
        }
        _ => {
            builder.session_cookie_from_default_locations()?;
        }
    }

    match ((args.year, args.day), (config.year, config.day)) {
        // Specific Year, Specific Day
        ((Some(year), Some(day)), (_, _))
        | ((Some(year), None), (_, ConfigPuzzleDay::Day(day)))
        | ((None, Some(day)), (ConfigPuzzleYear::Year(year), _))
        | (
            (None, None),
            (ConfigPuzzleYear::Year(year), ConfigPuzzleDay::Day(day)),
        ) => {
            builder.year(year)?.day(day)?;
        }
        // Specific Year, Latest Day
        ((Some(year), None), (_, ConfigPuzzleDay::LatestDay))
        | (
            (None, None),
            (ConfigPuzzleYear::Year(year), ConfigPuzzleDay::LatestDay),
        ) => {
            builder.year(year)?.latest_puzzle_day()?;
        }
        // Latest Year, Specific Day
        ((None, Some(day)), (ConfigPuzzleYear::LatestYear, _))
        | (
            (None, None),
            (ConfigPuzzleYear::LatestYear, ConfigPuzzleDay::Day(day)),
        ) => {
            builder.latest_event_year()?.day(day)?;
        }
        // Latest Year, Latest Day
        (
            (None, None),
            (ConfigPuzzleYear::LatestYear, ConfigPuzzleDay::LatestDay),
        ) => {
            builder.latest_event_year()?.latest_puzzle_day()?;
        }
    }

    match (args.width, config.width) {
        (Some(width), _) | (_, Some(width)) => {
            builder.output_width(width)?;
        }
        _ => {}
    }

    // use the config file name only if the user didn't specify another file name via args
    match (
        config.input_filename,
        args.input_file.eq(DEFAULT_PUZZLE_INPUT),
    ) {
        (Some(input_file), true) => {
            builder.input_filename(&input_file);
        }
        _ => {
            builder.input_filename(&args.input_file);
        }
    }

    // use the config file name only if the user didn't specify another file name via args
    match (
        config.description_filename,
        args.puzzle_file.eq(DEFAULT_PUZZLE_DESCRIPTION),
    ) {
        (Some(input_file), true) => {
            builder.puzzle_filename(&input_file);
        }
        _ => {
            builder.puzzle_filename(&args.puzzle_file);
        }
    }

    builder
        .overwrite_files(args.overwrite)
        .show_html_markup(args.show_html_markup)
        .leaderboard_id(config.private_leaderboard_id)
        .build()
}

fn run(args: &Args, client: AocClient) -> AocResult<()> {
    match &args.command {
        Some(Command::Calendar) => client.show_calendar(),
        Some(Command::Download) => {
            if !args.input_only {
                client.save_puzzle_markdown()?;
            }
            if !args.puzzle_only {
                client.save_input()?;
            }
            Ok(())
        }
        Some(Command::Init) => client.user_init_config(),
        Some(Command::Submit { part, answer }) => {
            client.submit_answer_and_show_outcome(part, answer)
        }
        Some(Command::PrivateLeaderboard { leaderboard_id }) => {
            client.show_private_leaderboard(*leaderboard_id)
        }
        _ => client.show_puzzle(),
    }
}
