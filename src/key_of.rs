use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn unix_absolute(path: &str) -> bool {
    path.starts_with('/')
}

fn windows_absolute(path: &str) -> bool {
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
    absolute: fn(&str) -> bool,
) -> Result<&'a str, String> {
    match present(value) {
        None => Err(format!("{name} is not set")),
        Some(path) if !absolute(path) => Err(format!("{name} is relative")),
        Some(path) => Ok(path),
    }
}

pub fn key_path_of(
    windows: bool,
    appdata: Option<&str>,
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, String> {
    let folder = if windows {
        PathBuf::from(folder_of("APPDATA", appdata, windows_absolute)?)
    } else {
        match (
            folder_of("XDG_CONFIG_HOME", xdg_config_home, unix_absolute),
            folder_of("HOME", home, unix_absolute),
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

pub fn bare_key_of(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text).trim()
}

pub fn header_key_of(key: &str) -> Result<String, String> {
    if key.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        Ok(key.to_string())
    } else {
        Err("the API key contains characters an HTTP header cannot carry".to_string())
    }
}

pub fn key_of(environment: Option<&str>, stored: Option<&str>) -> Result<String, String> {
    let key = [environment, stored]
        .into_iter()
        .flatten()
        .map(bare_key_of)
        .find(|key| !key.is_empty())
        .ok_or_else(|| "no API key: set TYPESAFE_API_KEY or run `pyes auth`".to_string())?;

    header_key_of(key)
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

    file.write_all(key.as_bytes()).map_err(failed)?;

    #[cfg(unix)]
    fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .map_err(failed)?;

    Ok(())
}

#[cfg(test)]
#[path = "key_of.test.rs"]
mod tests;
