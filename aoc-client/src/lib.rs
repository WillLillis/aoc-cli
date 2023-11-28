use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone, Utc};
use colored::{Color, Colorize};
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use dirs::config_dir;
use home::home_dir;
use html2md::parse_html;
use html2text::{
    from_read, from_read_with_decorator,
    render::text_renderer::TrivialDecorator,
};
use http::StatusCode;
use log::{debug, error, info, warn};
use regex::Regex;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::{
    HeaderMap, HeaderValue, CONTENT_TYPE, COOKIE, USER_AGENT,
};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, Reverse};
use std::collections::HashMap;
use std::env;
use std::fmt::{self, Display, Formatter};
use std::fs::{read_to_string, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::string::ToString;
use thiserror::Error;

pub type PuzzleYear = i32;
pub type PuzzleDay = u32;
pub type LeaderboardId = u32;
type MemberId = u64;
type Score = u64;

#[derive(Debug)]
pub enum PuzzlePart {
    PartOne,
    PartTwo,
}

#[derive(Debug)]
pub enum SubmissionOutcome {
    Correct,
    Incorrect,
    Wait,
    WrongLevel,
}

const FIRST_EVENT_YEAR: PuzzleYear = 2015;
const DECEMBER: u32 = 12;
const FIRST_PUZZLE_DAY: PuzzleDay = 1;
const LAST_PUZZLE_DAY: PuzzleDay = 25;
const RELEASE_TIMEZONE_OFFSET: i32 = -5 * 3600;

const SESSION_COOKIE_FILE: &str = "adventofcode.session";
const HIDDEN_SESSION_COOKIE_FILE: &str = ".adventofcode.session";
const SESSION_COOKIE_ENV_VAR: &str = "ADVENT_OF_CODE_SESSION";

pub const DEFAULT_PUZZLE_INPUT: &str = "input";
pub const DEFAULT_PUZZLE_DESCRIPTION: &str = "puzzle.md";

const CONFIG_FILE: &str = ".adventofcode_config.toml";

const DEFAULT_COL_WIDTH: usize = 80;

const PKG_REPO: &str = env!("CARGO_PKG_REPOSITORY");
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

const GOLD: Color = Color::Yellow;
const SILVER: Color = Color::TrueColor {
    r: 160,
    g: 160,
    b: 160,
};
const DARK_GRAY: Color = Color::TrueColor {
    r: 96,
    g: 96,
    b: 96,
};

pub type AocResult<T> = Result<T, AocError>;

#[derive(Error, Debug)]
pub enum AocError {
    #[error("Invalid puzzle date: day {0}, year {1}")]
    InvalidPuzzleDate(PuzzleDay, PuzzleYear),

    #[error("{0} is not a valid Advent of Code year")]
    InvalidEventYear(PuzzleYear),

    #[error("{0} is not a valid Advent of Code day")]
    InvalidPuzzleDay(PuzzleDay),

    #[error("Puzzle {0} of {1} is still locked")]
    LockedPuzzle(PuzzleDay, PuzzleYear),

    #[error("Session cookie file not found in home or config directory")]
    SessionFileNotFound,

    #[error("Failed to read session cookie from '{filename}': {source}")]
    SessionFileReadError {
        filename: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Invalid session cookie")]
    InvalidSessionCookie,

    #[error("HTTP request error: {0}")]
    HttpRequestError(#[from] reqwest::Error),

    #[error("Failed to parse Advent of Code response")]
    AocResponseError,

    #[error("The private leaderboard does not exist or you are not a member")]
    PrivateLeaderboardNotAvailable,

    #[error("No private leaderboard id. \
        Provide an argument (e.g. \"aoc private-leaderboard <LEADERBOARD-ID>\") \
        or add it to your config file.")]
    PrivateLeaderboardNoId,

    #[error("Failed to write to file '{filename}': {source}")]
    FileWriteError {
        filename: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Configuration file error: {0}")]
    ConfigError(String),

    #[error("Failed to create client due to missing field: {0}")]
    ClientFieldMissing(String),

    #[error("Invalid puzzle part number")]
    InvalidPuzzlePart,

    #[error("Output width must be greater than zero")]
    InvalidOutputWidth,
}

pub struct AocClient {
    session_cookie: String,
    unlock_datetime: DateTime<FixedOffset>,
    year: PuzzleYear,
    day: PuzzleDay,
    output_width: usize,
    overwrite_files: bool,
    input_filename: PathBuf,
    puzzle_filename: PathBuf,
    show_html_markup: bool,
    leaderboard_id: Option<LeaderboardId>,
}

#[must_use]
pub struct AocClientBuilder {
    session_cookie: Option<String>,
    year: Option<PuzzleYear>,
    day: Option<PuzzleDay>,
    output_width: usize,
    overwrite_files: bool,
    input_filename: PathBuf,
    puzzle_filename: PathBuf,
    show_html_markup: bool,
    leaderboard_id: Option<LeaderboardId>,
}

impl AocClient {
    pub fn builder() -> AocClientBuilder {
        AocClientBuilder::default()
    }

    pub fn get_config() -> (Config, Option<PathBuf>) {
        if let Some(home_dir) = home_dir() {
            let mut config_path = home_dir.clone();
            config_path.push(CONFIG_FILE);
            if let Ok(config_contents) = std::fs::read_to_string(&config_path) {
                match toml::from_str(&config_contents) {
                    Ok(contents) => {
                        debug!(
                            "🦌 Config file loaded: {}",
                            config_path.display()
                        );
                        return (contents, Some(config_path));
                    }
                    Err(e) => {
                        warn!(
                            "Failed to deserialize config file -- Error: {e}"
                        );
                        return (Config::default(), None);
                    }
                }
            }
        }
        if let Some(config_dir) = config_dir() {
            let mut config_path = config_dir.clone();
            config_path.push(CONFIG_FILE);
            if let Ok(config_contents) = std::fs::read_to_string(&config_path) {
                match toml::from_str(&config_contents) {
                    Ok(contents) => {
                        debug!(
                            "🦌 Config file loaded: {}",
                            config_path.display()
                        );
                        return (contents, Some(config_path));
                    }
                    Err(e) => {
                        warn!(
                            "Failed to deserialize config file -- Error: {e}"
                        );
                        return (Config::default(), None);
                    }
                }
            }
        }

        (Config::default(), None)
    }

    pub fn day_unlocked(&self) -> bool {
        let timezone = FixedOffset::east_opt(RELEASE_TIMEZONE_OFFSET).unwrap();
        let now = timezone.from_utc_datetime(&Utc::now().naive_utc());
        now.signed_duration_since(self.unlock_datetime)
            .num_milliseconds()
            >= 0
    }

    fn ensure_day_unlocked(&self) -> AocResult<()> {
        if self.day_unlocked() {
            Ok(())
        } else {
            Err(AocError::LockedPuzzle(self.day, self.year))
        }
    }

    pub fn get_puzzle_html(&self) -> AocResult<String> {
        self.ensure_day_unlocked()?;

        debug!("🦌 Fetching puzzle for day {}, {}", self.day, self.year);

        let url =
            format!("https://adventofcode.com/{}/day/{}", self.year, self.day);
        let response = http_client(&self.session_cookie, "text/html")?
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())?;
        let puzzle_html = Regex::new(r"(?i)(?s)<main>(?P<main>.*)</main>")
            .unwrap()
            .captures(&response)
            .ok_or(AocError::AocResponseError)?
            .name("main")
            .unwrap()
            .as_str()
            .to_string();

        Ok(puzzle_html)
    }

    pub fn get_input(&self) -> AocResult<String> {
        self.ensure_day_unlocked()?;

        debug!("🦌 Fetching input for day {}, {}", self.day, self.year);

        let url = format!(
            "https://adventofcode.com/{}/day/{}/input",
            self.year, self.day
        );
        http_client(&self.session_cookie, "text/plain")?
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
            .map_err(AocError::from)
    }

    fn submit_answer_html<P, D>(
        &self,
        puzzle_part: P,
        answer: D,
    ) -> AocResult<String>
    where
        P: TryInto<PuzzlePart>,
        AocError: From<P::Error>,
        D: Display,
    {
        self.ensure_day_unlocked()?;
        let part: PuzzlePart = puzzle_part.try_into()?;

        debug!(
            "🦌 Submitting answer for part {part}, day {}, {}",
            self.day, self.year
        );

        let url = format!(
            "https://adventofcode.com/{}/day/{}/answer",
            self.year, self.day
        );
        let content_type = "application/x-www-form-urlencoded";
        let response = http_client(&self.session_cookie, content_type)?
            .post(url)
            .body(format!("level={part}&answer={answer}"))
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
            .map_err(AocError::HttpRequestError)?;

        let outcome_html = Regex::new(r"(?i)(?s)<main>(?P<main>.*)</main>")
            .unwrap()
            .captures(&response)
            .ok_or(AocError::AocResponseError)?
            .name("main")
            .unwrap()
            .as_str()
            .to_string();

        Ok(outcome_html)
    }

    pub fn submit_answer<P, D>(
        &self,
        puzzle_part: P,
        answer: D,
    ) -> AocResult<SubmissionOutcome>
    where
        P: TryInto<PuzzlePart>,
        AocError: From<P::Error>,
        D: Display,
    {
        let outcome = self.submit_answer_html(puzzle_part, answer)?;
        if outcome.contains("That's the right answer") {
            Ok(SubmissionOutcome::Correct)
        } else if outcome.contains("That's not the right answer") {
            Ok(SubmissionOutcome::Incorrect)
        } else if outcome.contains("You gave an answer too recently") {
            Ok(SubmissionOutcome::Wait)
        } else if outcome
            .contains("You don't seem to be solving the right level")
        {
            Ok(SubmissionOutcome::WrongLevel)
        } else {
            Err(AocError::AocResponseError)
        }
    }

    pub fn submit_answer_and_show_outcome<P, D>(
        &self,
        puzzle_part: P,
        answer: D,
    ) -> AocResult<()>
    where
        P: TryInto<PuzzlePart>,
        AocError: From<P::Error>,
        D: Display,
    {
        let outcome_html = self.submit_answer_html(puzzle_part, answer)?;
        println!("\n{}", self.html2text(&outcome_html));
        Ok(())
    }

    pub fn show_puzzle(&self) -> AocResult<()> {
        let puzzle_html = self.get_puzzle_html()?;
        println!("\n{}", self.html2text(&puzzle_html));
        Ok(())
    }

    pub fn save_puzzle_markdown(&self) -> AocResult<()> {
        let puzzle_html = self.get_puzzle_html()?;
        let puzzle_markdow = parse_html(&puzzle_html);
        save_file(
            &self.puzzle_filename,
            self.overwrite_files,
            &puzzle_markdow,
        )?;
        info!("🎅 Saved puzzle to '{}'", self.puzzle_filename.display());
        Ok(())
    }

    pub fn save_input(&self) -> AocResult<()> {
        let input = self.get_input()?;
        save_file(&self.input_filename, self.overwrite_files, &input)?;
        info!("🎅 Saved input to '{}'", self.input_filename.display());
        Ok(())
    }

    pub fn get_calendar_html(&self) -> AocResult<String> {
        debug!("🦌 Fetching {} calendar", self.year);

        let url = format!("https://adventofcode.com/{}", self.year);
        let response = http_client(&self.session_cookie, "text/html")?
            .get(url)
            .send()?;

        if response.status() == StatusCode::NOT_FOUND {
            // A 402 reponse means the calendar for
            // the requested year is not yet available
            return Err(AocError::InvalidEventYear(self.year));
        }

        let contents = response.error_for_status()?.text()?;

        if Regex::new(r#"href="/[0-9]{4}/auth/login""#)
            .unwrap()
            .is_match(&contents)
        {
            warn!(
                "🍪 It looks like you are not logged in, try logging in again"
            );
        }

        let main = Regex::new(r"(?i)(?s)<main>(?P<main>.*)</main>")
            .unwrap()
            .captures(&contents)
            .ok_or(AocError::AocResponseError)?
            .name("main")
            .unwrap()
            .as_str()
            .to_string();

        // Remove elements that won't render well in the terminal
        let cleaned_up = Regex::new(concat!(
            // Remove 2015 "calendar-bkg"
            r#"(<div class="calendar-bkg">[[:space:]]*"#,
            r#"(<div>[^<]*</div>[[:space:]]*)*</div>)"#,
            // Remove 2017 "naughty/nice" animation
            r#"|(<div class="calendar-printer">(?s:.)*"#,
            r#"\|O\|</span></div>[[:space:]]*)"#,
            // Remove 2018 "space mug"
            r#"|(<pre id="spacemug"[^>]*>[^<]*</pre>)"#,
            // Remove 2019 shadows
            r#"|(<span style="color[^>]*position:absolute"#,
            r#"[^>]*>\.</span>)"#,
            // Remove 2019 "sunbeam"
            r#"|(<span class="sunbeam"[^>]*>"#,
            r#"<span style="animation-delay[^>]*>\*</span></span>)"#,
        ))
        .unwrap()
        .replace_all(&main, "")
        .to_string();

        let class_regex =
            Regex::new(r#"<a [^>]*class="(?P<class>[^"]*)""#).unwrap();
        let star_regex = Regex::new(concat!(
            r#"(?P<stars><span class="calendar-mark-complete">\*</span>"#,
            r#"<span class="calendar-mark-verycomplete">\*</span>)"#,
        ))
        .unwrap();

        // Remove stars that have not been collected
        let calendar = cleaned_up
            .lines()
            .map(|line| {
                let class = class_regex
                    .captures(line)
                    .and_then(|c| c.name("class"))
                    .map(|c| c.as_str())
                    .unwrap_or("");

                let stars = if class.contains("calendar-verycomplete") {
                    "**"
                } else if class.contains("calendar-complete") {
                    "*"
                } else {
                    ""
                };

                star_regex.replace(line, stars)
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(calendar)
    }

    pub fn show_calendar(&self) -> AocResult<()> {
        let calendar_html = self.get_calendar_html()?;
        let calendar_text = from_read_with_decorator(
            calendar_html.as_bytes(),
            self.output_width,
            TrivialDecorator::new(),
        );
        println!("\n{calendar_text}");
        Ok(())
    }

    pub fn write_config(
        &self,
        config: Config,
        path: &str,
        overwrite: bool,
    ) -> AocResult<()> {
        let config_str = match toml::to_string(&config) {
            Ok(ser) => ser,
            Err(e) => {
                return Err(AocError::ConfigError(format!(
                    "Failed to serialize `Config` struct -- Error: {e}"
                )));
            }
        };

        save_file(path, overwrite, &config_str)?;
        Ok(())
    }

    fn prompt_user_config(&self) -> AocResult<Config> {
        let latest_year = last_unlocked_year();
        let config_year: ConfigOption<PuzzleYear> = Input::with_theme(
            &ColorfulTheme::default(),
        )
        .with_prompt("Puzzle Year (empty for default)")
        .default(ConfigOption { inner: None })
        .validate_with({
            move |input: &ConfigOption<PuzzleYear>| -> Result<(), String> {
                match input.inner {
                    None => Ok(()),
                    Some(ref year) => {
                        if (FIRST_EVENT_YEAR..=latest_year).contains(year) {
                            Ok(())
                        } else {
                            let err = format!(
                                "Year must lie in the range {}-{}",
                                FIRST_EVENT_YEAR, latest_year
                            );
                            Err(err)
                        }
                    }
                }
            }
        })
        .interact_text()
        .unwrap();

        let config_day: ConfigOption<PuzzleDay> = Input::with_theme(
            &ColorfulTheme::default(),
        )
        .with_prompt("Puzzle Day (empty for default)")
        .default(ConfigOption { inner: None })
        .validate_with({
            move |input: &ConfigOption<PuzzleDay>| -> Result<(), String> {
                match input.inner {
                    None => Ok(()),
                    Some(ref day) => {
                        if (FIRST_PUZZLE_DAY..=LAST_PUZZLE_DAY).contains(day) {
                            Ok(())
                        } else {
                            let err = format!(
                                "Day must lie in the range {}-{}",
                                FIRST_PUZZLE_DAY, LAST_PUZZLE_DAY
                            );
                            Err(err)
                        }
                    }
                }
            }
        })
        .interact_text()
        .unwrap();

        let mut home_path = match home_dir() {
            Some(path) => path,
            None => {
                return Err(AocError::ConfigError(String::from(
                    "Failed to resolve home directory",
                )));
            }
        };
        home_path.push(HIDDEN_SESSION_COOKIE_FILE);
        let default_path = home_path.into_os_string().into_string().unwrap();

        let config_session_file: ConfigOption<String> =
            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Session File")
                .default(ConfigOption {
                    inner: Some(default_path.clone()),
                })
                .interact_text()
                .unwrap();

        let config_width: ConfigOption<usize> =
            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Column width")
                .default(ConfigOption {
                    inner: Some(DEFAULT_COL_WIDTH),
                })
                .validate_with({
                    move |input: &ConfigOption<usize>| -> Result<(), &str> {
                        match input.inner {
                            None => Ok(()),
                            Some(width) => {
                                if width > 0 {
                                    Ok(())
                                } else {
                                    Err("Column width must be greater than 0")
                                }
                            }
                        }
                    }
                })
                .interact_text()
                .unwrap();

        let config_input_filename: ConfigOption<String> =
            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Puzzle input filename")
                .default(ConfigOption {
                    inner: Some(String::from(DEFAULT_PUZZLE_INPUT)),
                })
                .interact_text()
                .unwrap();

        let config_description_filename: ConfigOption<String> =
            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Puzzle description filename")
                .default(ConfigOption {
                    inner: Some(String::from(DEFAULT_PUZZLE_DESCRIPTION)),
                })
                .interact_text()
                .unwrap();

        let config_leaderboard_id: ConfigOption<LeaderboardId> =
            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Private leaderboard ID")
                .default(ConfigOption { inner: None })
                .interact_text()
                .unwrap();

        Ok(Config {
            year: config_year,
            day: config_day,
            session_file: config_session_file,
            width: config_width,
            input_filename: config_input_filename,
            description_filename: config_description_filename,
            private_leaderboard_id: config_leaderboard_id,
        })
    }

    pub fn user_init_config(&self) -> AocResult<()> {
        let mut home_path = match home_dir() {
            Some(path) => path,
            None => {
                return Err(AocError::ConfigError(String::from(
                    "Failed to resolve home directory",
                )));
            }
        };
        home_path.push(CONFIG_FILE);
        let home_option = home_path.into_os_string().into_string().unwrap();

        let mut config_path = match config_dir() {
            Some(path) => path,
            None => {
                return Err(AocError::ConfigError(String::from(
                    "Failed to resolve .config directory",
                )));
            }
        };
        config_path.push(CONFIG_FILE);
        let config_option = config_path.into_os_string().into_string().unwrap();

        let save_options = &[home_option, config_option];

        let save_location = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Config file location")
            .default(0)
            .items(&save_options[..])
            .interact()
            .unwrap();

        let config = self.prompt_user_config()?;

        self.write_config(
            config,
            &save_options[save_location],
            self.overwrite_files,
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_config(
        &self,
        year: Option<PuzzleYear>,
        day: Option<PuzzleDay>,
        session_filename: &Option<String>,
        width: Option<usize>,
        input_filename: &Option<String>,
        description_filename: &Option<String>,
        private_leaderboard_id: Option<LeaderboardId>,
    ) -> AocResult<()> {
        let mut config;
        let config_path;

        match AocClient::get_config() {
            (_, None) => {
                return Err(AocError::ConfigError(String::from(
                    "Failed to find/ read in existing config file. \
                    Instantiate a fresh config file using `aoc init-config`",
                )));
            }
            (config_in, Some(path)) => {
                config = config_in;
                config_path = path;
            }
        }

        debug!("Old config:\n{:#?}", config);

        if let Some(new_year) = year {
            config.year = ConfigOption::new(Some(new_year));
        }
        if let Some(new_day) = day {
            config.day = ConfigOption::new(Some(new_day));
        }
        if let Some(session_file) = session_filename {
            config.session_file = ConfigOption::new(Some(session_file.clone()));
        }
        if let Some(new_width) = width {
            config.width = ConfigOption::new(Some(new_width));
        }
        if let Some(input_file) = input_filename {
            config.input_filename = ConfigOption::new(Some(input_file.clone()));
        }
        if let Some(description_file) = description_filename {
            config.description_filename =
                ConfigOption::new(Some(description_file.clone()));
        }
        if let Some(leaderboard_id) = private_leaderboard_id {
            config.private_leaderboard_id =
                ConfigOption::new(Some(leaderboard_id));
        }

        debug!("Updated config:\n{:#?}", config);

        self.write_config(
            config,
            config_path.into_os_string().to_str().unwrap(),
            true,
        )?;

        Ok(())
    }

    fn get_private_leaderboard(
        &self,
        leaderboard_id: LeaderboardId,
    ) -> AocResult<PrivateLeaderboard> {
        debug!("🦌 Fetching private leaderboard {leaderboard_id}");

        let url = format!(
            "https://adventofcode.com/{}/leaderboard/private/view\
            /{leaderboard_id}.json",
            self.year,
        );
        let response = http_client(&self.session_cookie, "application/json")?
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())?;

        if response.status() == StatusCode::FOUND {
            // A 302 reponse is a redirect and it means
            // the leaderboard doesn't exist or we can't access it
            return Err(AocError::PrivateLeaderboardNotAvailable);
        }

        response.json().map_err(AocError::from)
    }

    pub fn show_private_leaderboard(
        &self,
        leaderboard_id: Option<LeaderboardId>,
    ) -> AocResult<()> {
        let last_unlocked_day = last_unlocked_day(self.year)
            .ok_or(AocError::InvalidEventYear(self.year))?;

        let leaderboard_id = match leaderboard_id {
            Some(id) => id, // passed in as arg
            None => {
                if let Some(id) = self.leaderboard_id {
                    id // from config file
                } else {
                    return Err(AocError::PrivateLeaderboardNoId);
                }
            }
        };
        let leaderboard = self.get_private_leaderboard(leaderboard_id)?;
        let owner_name = leaderboard
            .get_owner_name()
            .ok_or(AocError::AocResponseError)?;

        println!(
            "Private leaderboard of {} for Advent of Code {}.\n\n\
            {} indicates the user got both stars for that day,\n\
            {} means just the first star, and a {} means none.\n",
            owner_name.bold(),
            self.year.to_string().bold(),
            "Gold *".color(GOLD),
            "silver *".color(SILVER),
            "gray dot (.)".color(DARK_GRAY),
        );

        let mut members: Vec<_> = leaderboard.members.values().collect();
        members.sort_by_key(|member| Reverse(*member));

        let highest_score = members.first().map(|m| m.local_score).unwrap_or(0);
        let score_width = highest_score.to_string().len();
        let highest_rank = 1 + leaderboard.members.len();
        let rank_width = highest_rank.to_string().len();
        let header_pad: String =
            vec![' '; rank_width + score_width].into_iter().collect();

        for header in ["         1111111111222222", "1234567890123456789012345"]
        {
            let (on, off) = header.split_at(last_unlocked_day as usize);
            println!("{header_pad}   {}{}", on, off.color(DARK_GRAY));
        }

        for (member, rank) in members.iter().zip(1..) {
            let stars: String = (FIRST_PUZZLE_DAY..=LAST_PUZZLE_DAY)
                .map(|day| {
                    if day > last_unlocked_day {
                        " ".normal()
                    } else {
                        match member.count_stars(day) {
                            2 => "*".color(GOLD),
                            1 => "*".color(SILVER),
                            _ => ".".color(DARK_GRAY),
                        }
                    }
                    .to_string()
                })
                .collect();

            println!(
                "{rank:rank_width$}) {:score_width$} {stars}  {}",
                member.local_score,
                member.get_name(),
            );
        }

        Ok(())
    }

    fn html2text(&self, html: &str) -> String {
        if self.show_html_markup {
            from_read(html.as_bytes(), self.output_width)
        } else {
            from_read_with_decorator(
                html.as_bytes(),
                self.output_width,
                TrivialDecorator::new(),
            )
        }
    }
}

impl Default for AocClientBuilder {
    fn default() -> Self {
        let session_cookie = None;
        let year = None;
        let day = None;
        let output_width = term_size::dimensions()
            .map(|(w, _)| w)
            .unwrap_or(DEFAULT_COL_WIDTH);
        let overwrite_files = false;
        let input_filename = "input".into();
        let puzzle_filename = "puzzle.md".into();
        let show_html_markup = false;
        let leaderboard_id = None;

        Self {
            session_cookie,
            year,
            day,
            output_width,
            overwrite_files,
            input_filename,
            puzzle_filename,
            show_html_markup,
            leaderboard_id,
        }
    }
}

impl AocClientBuilder {
    pub fn build(&self) -> AocResult<AocClient> {
        for (missing, field) in [
            (self.session_cookie.is_none(), "session cookie"),
            (self.year.is_none(), "year"),
            (self.day.is_none(), "day"),
        ] {
            if missing {
                return Err(AocError::ClientFieldMissing(field.to_string()));
            }
        }

        let day = self.day.unwrap();
        let year = self.year.unwrap();
        let timezone = FixedOffset::east_opt(RELEASE_TIMEZONE_OFFSET).unwrap();
        let local_datetime = NaiveDate::from_ymd_opt(year, DECEMBER, day)
            .ok_or(AocError::InvalidPuzzleDate(day, year))?
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let unlock_datetime = timezone
            .from_local_datetime(&local_datetime)
            .single()
            .ok_or(AocError::InvalidPuzzleDate(day, year))?;

        Ok(AocClient {
            session_cookie: self.session_cookie.clone().unwrap(),
            unlock_datetime,
            year: self.year.unwrap(),
            day: self.day.unwrap(),
            output_width: self.output_width,
            overwrite_files: self.overwrite_files,
            input_filename: self.input_filename.clone(),
            puzzle_filename: self.puzzle_filename.clone(),
            show_html_markup: self.show_html_markup,
            leaderboard_id: self.leaderboard_id,
        })
    }

    pub fn session_cookie(
        &mut self,
        session_cookie: impl AsRef<str>,
    ) -> AocResult<&mut Self> {
        let cookie = session_cookie.as_ref().trim();
        if cookie.is_empty() || !cookie.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AocError::InvalidSessionCookie);
        }
        self.session_cookie = Some(cookie.to_string());
        Ok(self)
    }

    pub fn session_cookie_from_default_locations(
        &mut self,
    ) -> AocResult<&mut Self> {
        if let Ok(cookie) = env::var(SESSION_COOKIE_ENV_VAR) {
            if !cookie.trim().is_empty() {
                debug!(
                    "🍪 Loading session cookie from '{SESSION_COOKIE_ENV_VAR}' \
                    environment variable"
                );

                return self.session_cookie(&cookie);
            }

            warn!(
                "🍪 Environment variable '{SESSION_COOKIE_ENV_VAR}' is set \
                but it is empty, ignoring"
            );
        }

        let path = if let Some(home_path) = home_dir()
            .map(|dir| dir.join(HIDDEN_SESSION_COOKIE_FILE))
            .filter(|file| file.exists())
        {
            home_path
        } else if let Some(config_path) = config_dir()
            .map(|dir| dir.join(SESSION_COOKIE_FILE))
            .filter(|file| file.exists())
        {
            config_path
        } else {
            return Err(AocError::SessionFileNotFound);
        };

        self.session_cookie_from_file(path)
    }

    pub fn session_cookie_from_file<P: AsRef<Path>>(
        &mut self,
        file: P,
    ) -> AocResult<&mut Self> {
        let cookie = read_to_string(&file).map_err(|err| {
            AocError::SessionFileReadError {
                filename: file.as_ref().display().to_string(),
                source: err,
            }
        })?;

        debug!(
            "🍪 Loading session cookie from '{}'",
            file.as_ref().display()
        );
        self.session_cookie(&cookie)
    }

    pub fn year(&mut self, year: PuzzleYear) -> AocResult<&mut Self> {
        if year >= FIRST_EVENT_YEAR {
            self.year = Some(year);
            Ok(self)
        } else {
            Err(AocError::InvalidEventYear(year))
        }
    }

    pub fn latest_event_year(&mut self) -> AocResult<&mut Self> {
        self.year(last_unlocked_year())
    }

    pub fn day(&mut self, day: PuzzleDay) -> AocResult<&mut Self> {
        if (FIRST_PUZZLE_DAY..=LAST_PUZZLE_DAY).contains(&day) {
            self.day = Some(day);
            Ok(self)
        } else {
            Err(AocError::InvalidPuzzleDay(day))
        }
    }

    pub fn latest_puzzle_day(&mut self) -> AocResult<&mut Self> {
        if self.year.is_none() {
            self.latest_event_year()?;
        }

        let event_year = self.year.unwrap();
        let now = FixedOffset::east_opt(RELEASE_TIMEZONE_OFFSET)
            .unwrap()
            .from_utc_datetime(&Utc::now().naive_utc());

        if event_year == now.year() && now.month() == DECEMBER {
            if now.day() <= LAST_PUZZLE_DAY {
                self.day(now.day())
            } else {
                self.day(LAST_PUZZLE_DAY)
            }
        } else if event_year < now.year() {
            // For past events, return the last puzzle day
            self.day(LAST_PUZZLE_DAY)
        } else {
            // For future events, return the first puzzle day
            self.day(FIRST_PUZZLE_DAY)
        }
    }

    pub fn output_width(&mut self, width: usize) -> AocResult<&mut Self> {
        if width > 0 {
            self.output_width = width;
            Ok(self)
        } else {
            Err(AocError::InvalidOutputWidth)
        }
    }

    pub fn overwrite_files(&mut self, overwrite: bool) -> &mut Self {
        self.overwrite_files = overwrite;
        self
    }

    pub fn input_filename<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.input_filename = path.as_ref().into();
        self
    }

    pub fn puzzle_filename<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.puzzle_filename = path.as_ref().into();
        self
    }

    pub fn show_html_markup(&mut self, show: bool) -> &mut Self {
        self.show_html_markup = show;
        self
    }

    pub fn leaderboard_id(
        &mut self,
        leaderboard_id: Option<LeaderboardId>,
    ) -> &mut Self {
        self.leaderboard_id = leaderboard_id;
        self
    }
}

pub fn last_unlocked_day(year: PuzzleYear) -> Option<PuzzleDay> {
    let now = FixedOffset::east_opt(RELEASE_TIMEZONE_OFFSET)
        .unwrap()
        .from_utc_datetime(&Utc::now().naive_utc());

    if year == now.year() && now.month() == DECEMBER {
        if now.day() > LAST_PUZZLE_DAY {
            Some(LAST_PUZZLE_DAY)
        } else {
            Some(now.day())
        }
    } else if year >= FIRST_EVENT_YEAR && year < now.year() {
        Some(LAST_PUZZLE_DAY)
    } else {
        None
    }
}

pub fn last_unlocked_year() -> PuzzleYear {
    let now = FixedOffset::east_opt(RELEASE_TIMEZONE_OFFSET)
        .unwrap()
        .from_utc_datetime(&Utc::now().naive_utc());

    if now.month() < DECEMBER {
        now.year() - 1
    } else {
        now.year()
    }
}

fn http_client(
    session_cookie: &str,
    content_type: &str,
) -> AocResult<HttpClient> {
    let cookie_header =
        HeaderValue::from_str(&format!("session={}", session_cookie.trim()))
            .map_err(|_| AocError::InvalidSessionCookie)?;
    let content_type_header = HeaderValue::from_str(content_type).unwrap();
    let user_agent = format!("{PKG_REPO} {PKG_VERSION}");
    let user_agent_header = HeaderValue::from_str(&user_agent).unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, cookie_header);
    headers.insert(CONTENT_TYPE, content_type_header);
    headers.insert(USER_AGENT, user_agent_header);

    HttpClient::builder()
        .default_headers(headers)
        .redirect(Policy::none())
        .build()
        .map_err(AocError::from)
}

fn save_file<P: AsRef<Path>>(
    path: P,
    overwrite: bool,
    contents: &str,
) -> AocResult<()> {
    let mut file = OpenOptions::new();
    if overwrite {
        file.create(true);
    } else {
        file.create_new(true);
    };

    file.write(true)
        .truncate(true)
        .open(&path)
        .and_then(|mut file| file.write_all(contents.as_bytes()))
        .map_err(|err| AocError::FileWriteError {
            filename: path.as_ref().to_string_lossy().into(),
            source: err,
        })
}

#[derive(Deserialize)]
struct PrivateLeaderboard {
    owner_id: MemberId,
    members: HashMap<MemberId, Member>,
}

impl PrivateLeaderboard {
    fn get_owner_name(&self) -> Option<String> {
        self.members.get(&self.owner_id).map(|m| m.get_name())
    }
}

#[derive(Eq, Deserialize)]
struct Member {
    id: MemberId,
    name: Option<String>,
    local_score: Score,
    completion_day_level: HashMap<PuzzleDay, DayLevel>,
}

type DayLevel = HashMap<String, CollectedStar>;

#[derive(Eq, Deserialize, PartialEq)]
struct CollectedStar {}

impl Member {
    fn get_name(&self) -> String {
        self.name
            .as_ref()
            .cloned()
            .unwrap_or(format!("(anonymous user #{})", self.id))
    }

    fn count_stars(&self, day: PuzzleDay) -> usize {
        self.completion_day_level
            .get(&day)
            .map(|stars| stars.len())
            .unwrap_or(0)
    }
}

impl Ord for Member {
    fn cmp(&self, other: &Self) -> Ordering {
        // Members are sorted by increasing local score and then decreasing ID
        self.local_score
            .cmp(&other.local_score)
            .then(self.id.cmp(&other.id).reverse())
    }
}

impl PartialOrd for Member {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Member {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Display for PuzzlePart {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PartOne => write!(f, "1"),
            Self::PartTwo => write!(f, "2"),
        }
    }
}

impl TryFrom<&String> for PuzzlePart {
    type Error = AocError;

    fn try_from(s: &String) -> Result<Self, Self::Error> {
        s.as_str().try_into()
    }
}

impl TryFrom<&str> for PuzzlePart {
    type Error = AocError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "1" => Ok(Self::PartOne),
            "2" => Ok(Self::PartTwo),
            _ => Err(AocError::InvalidPuzzlePart),
        }
    }
}

impl TryFrom<i64> for PuzzlePart {
    type Error = AocError;

    fn try_from(n: i64) -> Result<Self, Self::Error> {
        match n {
            1 => Ok(Self::PartOne),
            2 => Ok(Self::PartTwo),
            _ => Err(AocError::InvalidPuzzlePart),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy)]
pub struct ConfigOption<T> {
    pub inner: Option<T>,
}

impl<T> FromStr for ConfigOption<T>
where
    T: FromStr,
{
    type Err = T::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            Ok(ConfigOption { inner: None })
        } else {
            match T::from_str(s) {
                Ok(item) => Ok(ConfigOption { inner: Some(item) }),
                Err(e) => Err(e),
            }
        }
    }
}

impl<T> Display for ConfigOption<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.inner {
            Some(ref inner) => write!(f, "{}", inner),
            None => write!(f, ""),
        }
    }
}

impl<T> ConfigOption<T> {
    fn new(a: Option<T>) -> Self {
        ConfigOption { inner: a }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub year: ConfigOption<PuzzleYear>,
    pub day: ConfigOption<PuzzleDay>,
    pub session_file: ConfigOption<String>,
    pub width: ConfigOption<usize>,
    pub input_filename: ConfigOption<String>,
    pub description_filename: ConfigOption<String>,
    pub private_leaderboard_id: ConfigOption<LeaderboardId>,
}
