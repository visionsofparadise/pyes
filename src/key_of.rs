use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

fn set_value_of(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn is_unix_absolute(path: &str) -> bool {
    path.starts_with('/')
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();

    path.starts_with(r"\\")
        || path.starts_with("//")
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

fn folder_of<'a>(
    name: &str,
    value: Option<&'a str>,
    not_unicode: &[&str],
    is_absolute: fn(&str) -> bool,
) -> Result<&'a str, String> {
    if not_unicode.contains(&name) {
        return Err(format!("{name} is not valid Unicode"));
    }

    match set_value_of(value) {
        None => Err(format!("{name} is not set")),
        Some(path) if !is_absolute(path) => Err(format!("{name} is relative")),
        Some(path) => Ok(path),
    }
}

pub fn key_path_of(
    windows: bool,
    appdata: Option<&str>,
    xdg_config_home: Option<&str>,
    home: Option<&str>,
    not_unicode: &[&str],
) -> Result<PathBuf, String> {
    let folder = if windows {
        PathBuf::from(folder_of(
            "APPDATA",
            appdata,
            not_unicode,
            is_windows_absolute,
        )?)
    } else {
        match (
            folder_of(
                "XDG_CONFIG_HOME",
                xdg_config_home,
                not_unicode,
                is_unix_absolute,
            ),
            folder_of("HOME", home, not_unicode, is_unix_absolute),
        ) {
            (Ok(xdg_config_home), _) => PathBuf::from(xdg_config_home),
            (_, Ok(home)) => PathBuf::from(home).join(".config"),
            (Err(xdg_config_home), Err(home)) => {
                return Err(format!("{xdg_config_home} and {home}"))
            }
        }
    };

    Ok(folder.join("pyes").join("key"))
}

pub fn stored_key_path_of(
    windows: bool,
    appdata: Option<&str>,
    xdg_config_home: Option<&str>,
    home: Option<&str>,
    not_unicode: &[&str],
) -> Result<Option<PathBuf>, String> {
    let is_unset = |name: &str, value: Option<&str>| {
        set_value_of(value).is_none() && !not_unicode.contains(&name)
    };
    let every_folder_unset = if windows {
        is_unset("APPDATA", appdata)
    } else {
        is_unset("XDG_CONFIG_HOME", xdg_config_home) && is_unset("HOME", home)
    };

    if every_folder_unset {
        return Ok(None);
    }

    key_path_of(windows, appdata, xdg_config_home, home, not_unicode).map(Some)
}

pub fn bare_key_of(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text).trim()
}

pub fn header_key_of(key: &str, source: &str) -> Result<String, String> {
    if key.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        Ok(key.to_string())
    } else {
        Err(format!(
            "{source} contains characters an HTTP header cannot carry"
        ))
    }
}

pub fn key_of(environment: Option<&str>, stored: Option<(&Path, &str)>) -> Result<String, String> {
    if let Some(key) = environment.map(bare_key_of).filter(|key| !key.is_empty()) {
        return header_key_of(key, "TYPESAFE_API_KEY");
    }

    match stored.map(|(path, text)| (path, bare_key_of(text))) {
        Some((path, key)) if !key.is_empty() => {
            header_key_of(key, &format!("the key in {}", path.display()))
        }
        _ => Err("no API key: set TYPESAFE_API_KEY or run `pyes auth`".to_string()),
    }
}

pub fn read_stored_key(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

pub fn store_key(path: &Path, key: &str) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("{}: {error}", path.display());

    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(failed)?;
    }

    let mut options = fs::OpenOptions::new();

    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);

    let mut file = options.open(path).map_err(failed)?;

    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .map_err(failed)?;

    file.write_all(key.as_bytes()).map_err(failed)?;

    Ok(())
}

#[cfg(test)]
#[path = "key_of.test.rs"]
mod tests;
