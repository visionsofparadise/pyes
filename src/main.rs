mod answers_of;
mod attempt;
mod chunks_of;
mod estimate_of;
mod key_of;
#[cfg(test)]
mod mock_jev;
mod parse_arguments;
mod request_of;
mod score_records;
mod split_records;
mod write_output;

use std::ffi::OsString;
use std::io::{BufWriter, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

use clap::error::ErrorKind;
use clap::Parser;

use attempt::{Client, BASE_URL};
use key_of::{key_of, key_path_of, read_stored_key, store_key};
use parse_arguments::{parse_arguments, Arguments, Command};
use score_records::score_records;
use split_records::{split_records, terminator_of, Separator};
use write_output::write_output;

const SUCCESS: i32 = 0;
const FAILURE: i32 = 2;

pub struct Environment {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub appdata: Option<String>,
    pub xdg_config_home: Option<String>,
    pub home: Option<String>,
}

pub struct Streams<'a> {
    pub stdin: &'a mut dyn Read,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
}

impl Streams<'_> {
    fn fail(&mut self, message: &str) -> i32 {
        let _ = writeln!(self.stderr, "pyes: {message}");

        FAILURE
    }
}

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn key_path_from(environment: &Environment) -> Result<std::path::PathBuf, String> {
    key_path_of(
        cfg!(windows),
        environment.appdata.as_deref(),
        environment.xdg_config_home.as_deref(),
        environment.home.as_deref(),
    )
}

fn authenticate(environment: &Environment, streams: &mut Streams) -> Result<(), String> {
    let mut input = String::new();

    streams
        .stdin
        .read_to_string(&mut input)
        .map_err(|error| format!("stdin: {error}"))?;

    let key = input.trim();

    if key.is_empty() {
        return Err("no key on stdin".to_string());
    }

    store_key(&key_path_from(environment)?, key)
}

fn score(
    questions: Vec<String>,
    separator: Separator,
    environment: &Environment,
    streams: &mut Streams,
) -> Result<(), String> {
    let mut input = Vec::new();

    streams
        .stdin
        .read_to_end(&mut input)
        .map_err(|error| format!("stdin: {error}"))?;

    if input.is_empty() {
        return Ok(());
    }

    let records = split_records(&input, &separator);
    let texts: Vec<String> = records
        .iter()
        .map(|record| String::from_utf8_lossy(record).into_owned())
        .collect();
    let stored = match present(environment.api_key.as_deref()) {
        Some(_) => None,
        None => read_stored_key(&key_path_from(environment)?)?,
    };
    let key = key_of(environment.api_key.as_deref(), stored.as_deref())?;
    let base_url = present(environment.base_url.as_deref()).unwrap_or(BASE_URL);
    let client = Client::new(base_url.to_string(), key);
    let columns = score_records(&client, &texts, &questions)?;
    let written = |error: std::io::Error| format!("stdout: {error}");
    let mut stdout = BufWriter::new(&mut *streams.stdout);

    write_output(&records, &columns, terminator_of(&separator), &mut stdout).map_err(written)?;

    stdout.flush().map_err(written)
}

fn run(arguments: Vec<OsString>, environment: &Environment, streams: &mut Streams) -> i32 {
    let arguments = match Arguments::try_parse_from(arguments) {
        Ok(arguments) => arguments,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            let _ = write!(streams.stdout, "{}", error.render());

            return SUCCESS;
        }
        Err(error) => {
            let rendered = error.render().to_string();
            let first_line = rendered.lines().next().unwrap_or_default();

            return streams.fail(first_line.trim_start_matches("error: "));
        }
    };

    let outcome = match parse_arguments(arguments) {
        Ok(Command::Auth) => authenticate(environment, streams),
        Ok(Command::Score {
            questions,
            separator,
        }) => score(questions, separator, environment, streams),
        Err(message) => Err(message),
    };

    match outcome {
        Ok(()) => SUCCESS,
        Err(message) => streams.fail(&message),
    }
}

fn main() {
    let variable_of = |name: &str| std::env::var(name).ok();
    let environment = Environment {
        api_key: variable_of("TYPESAFE_API_KEY"),
        base_url: variable_of("TYPESAFE_BASE_URL"),
        appdata: variable_of("APPDATA"),
        xdg_config_home: variable_of("XDG_CONFIG_HOME"),
        home: variable_of("HOME"),
    };
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr();

    let code = catch_unwind(AssertUnwindSafe(|| {
        run(
            std::env::args_os().collect(),
            &environment,
            &mut Streams {
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
            },
        )
    }))
    .unwrap_or(FAILURE);

    let _ = stdout.flush();

    std::process::exit(code);
}

#[cfg(test)]
#[path = "main.integration.test.rs"]
mod integration;
