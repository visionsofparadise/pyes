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

use std::env::VarError;
use std::ffi::OsString;
use std::io::{BufWriter, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

use clap::error::ErrorKind;
use clap::Parser;

use attempt::{Client, BASE_URL};
use key_of::{bare_key_of, header_key_of, key_of, key_path_of, read_stored_key, store_key};
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
    pub not_unicode: Vec<&'static str>,
}

impl Environment {
    fn require_unicode(&self, name: &str) -> Result<(), String> {
        if self.not_unicode.contains(&name) {
            return Err(format!("{name} is not valid Unicode"));
        }

        Ok(())
    }
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

fn closed(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::BrokenPipe
}

fn base_url_of(environment: &Environment) -> Result<&str, String> {
    environment.require_unicode("TYPESAFE_BASE_URL")?;

    Ok(present(environment.base_url.as_deref()).unwrap_or(BASE_URL))
}

fn key_path_from(environment: &Environment) -> Result<std::path::PathBuf, String> {
    key_path_of(
        cfg!(windows),
        environment.appdata.as_deref(),
        environment.xdg_config_home.as_deref(),
        environment.home.as_deref(),
        &environment.not_unicode,
    )
}

fn authenticate(environment: &Environment, streams: &mut Streams) -> Result<(), String> {
    let mut input = String::new();

    streams
        .stdin
        .read_to_string(&mut input)
        .map_err(|error| format!("stdin: {error}"))?;

    let key = bare_key_of(&input);

    if key.is_empty() {
        return Err("no key on stdin".to_string());
    }

    store_key(&key_path_from(environment)?, &header_key_of(key)?)
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

    environment.require_unicode("TYPESAFE_API_KEY")?;

    let stored = match present(environment.api_key.as_deref()) {
        Some(_) => None,
        None => match key_path_from(environment) {
            Ok(path) => read_stored_key(&path)?,
            Err(_) => None,
        },
    };
    let key = key_of(environment.api_key.as_deref(), stored.as_deref())?;
    let client = Client::new(base_url_of(environment)?.to_string(), key);
    let columns = score_records(&client, &texts, &questions)?;
    let mut stdout = BufWriter::new(&mut *streams.stdout);

    match write_output(&records, &columns, terminator_of(&separator), &mut stdout)
        .and_then(|()| stdout.flush())
    {
        Err(error) if !closed(&error) => Err(format!("stdout: {error}")),
        _ => Ok(()),
    }
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

fn environment_of(variable_of: impl Fn(&str) -> Result<String, VarError>) -> Environment {
    let mut not_unicode = Vec::new();
    let mut value_of = |name: &'static str| match variable_of(name) {
        Ok(value) => Some(value),
        Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(_)) => {
            not_unicode.push(name);

            None
        }
    };
    let api_key = value_of("TYPESAFE_API_KEY");
    let base_url = value_of("TYPESAFE_BASE_URL");
    let appdata = value_of("APPDATA");
    let xdg_config_home = value_of("XDG_CONFIG_HOME");
    let home = value_of("HOME");

    Environment {
        api_key,
        base_url,
        appdata,
        xdg_config_home,
        home,
        not_unicode,
    }
}

fn main() {
    let environment = environment_of(|name| std::env::var(name));
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr();

    std::panic::set_hook(Box::new(|_| {}));

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
    .unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("a panic ended the run");

        let _ = writeln!(stderr, "pyes: internal error: {message}");

        FAILURE
    });

    let _ = stdout.flush();

    std::process::exit(code);
}

#[cfg(test)]
#[path = "main.test.rs"]
mod tests;

#[cfg(test)]
#[path = "main.integration.test.rs"]
mod integration;
