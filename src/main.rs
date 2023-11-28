mod args;

use aoc_client::{
    AocClient, AocError, AocResult, ConfigOption, DEFAULT_PUZZLE_DESCRIPTION,
    DEFAULT_PUZZLE_INPUT,
};
use args::{Args, Command, SetConfig, UnsetConfig};
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
    let (config, _) = AocClient::get_config();

    match (&args.session_file, &config.session_file.inner) {
        (Some(ref file), _) | (_, Some(ref file)) => {
            builder.session_cookie_from_file(file)?;
        }
        _ => {
            builder.session_cookie_from_default_locations()?;
        }
    }

    // CLI args override config, if neither are provided use default (latest)
    match ((args.year, args.day), (config.year, config.day)) {
        // Specific Year, Specific Day
        ((Some(year), Some(day)), (_, _))
        | ((Some(year), None), (_, ConfigOption { inner: Some(day) }))
        | ((None, Some(day)), (ConfigOption { inner: Some(year) }, _))
        | (
            (None, None),
            (
                ConfigOption { inner: Some(year) },
                ConfigOption { inner: Some(day) },
            ),
        ) => {
            builder.year(year)?.day(day)?;
        }
        // Specific Year, Unspecified Day
        ((Some(year), None), (_, ConfigOption { inner: None }))
        | (
            (None, None),
            (ConfigOption { inner: Some(year) }, ConfigOption { inner: None }),
        ) => {
            builder.year(year)?.latest_puzzle_day()?;
        }
        // Unspecified Year, Specific Day
        ((None, Some(day)), (ConfigOption { inner: None }, _))
        | (
            (None, None),
            (ConfigOption { inner: None }, ConfigOption { inner: Some(day) }),
        ) => {
            builder.latest_event_year()?.day(day)?;
        }
        // Unspecified Year, Unspecified Day
        (
            (None, None),
            (ConfigOption { inner: None }, ConfigOption { inner: None }),
        ) => {
            builder.latest_event_year()?.latest_puzzle_day()?;
        }
    }

    match (args.width, config.width) {
        (Some(width), _) | (_, ConfigOption { inner: Some(width) }) => {
            builder.output_width(width)?;
        }
        _ => {}
    }

    // use the config file name only if the user didn't specify another file name via args
    match (
        config.input_filename,
        args.input_file.eq(DEFAULT_PUZZLE_INPUT),
    ) {
        (
            ConfigOption {
                inner: Some(input_file),
            },
            true,
        ) => {
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
        (
            ConfigOption {
                inner: Some(input_file),
            },
            true,
        ) => {
            builder.puzzle_filename(&input_file);
        }
        _ => {
            builder.puzzle_filename(&args.puzzle_file);
        }
    }

    let leaderboard_id = config.private_leaderboard_id.inner;
    builder
        .overwrite_files(args.overwrite)
        .show_html_markup(args.show_html_markup)
        .leaderboard_id(leaderboard_id)
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
        Some(Command::SetConfig(SetConfig {
            year,
            day,
            session_file,
            width,
            input_filename,
            description_filename,
            private_leaderboard_id,
        })) => client.set_config(
            *year,
            *day,
            session_file,
            *width,
            input_filename,
            description_filename,
            *private_leaderboard_id,
        ),
        Some(Command::UnsetConfig(UnsetConfig {
            unset_year,
            unset_day,
            unset_session_file,
            unset_width,
            unset_input_filename,
            unset_description_filename,
            unset_private_leaderboard_id,
        })) => client.unset_config(
            *unset_year,
            *unset_day,
            *unset_session_file,
            *unset_width,
            *unset_input_filename,
            *unset_description_filename,
            *unset_private_leaderboard_id,
        ),
        Some(Command::Submit { part, answer }) => {
            client.submit_answer_and_show_outcome(part, answer)
        }
        Some(Command::PrivateLeaderboard { leaderboard_id }) => {
            client.show_private_leaderboard(*leaderboard_id)
        }
        _ => client.show_puzzle(),
    }
}
