use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

pub fn key_path_of(
    windows: bool,
    appdata: Option<&str>,
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, String> {
    let folder = if windows {
        PathBuf::from(present(appdata).ok_or("APPDATA is not set")?)
    } else if let Some(xdg_config_home) = present(xdg_config_home) {
        PathBuf::from(xdg_config_home)
    } else {
        PathBuf::from(present(home).ok_or("neither XDG_CONFIG_HOME nor HOME is set")?)
            .join(".config")
    };

    Ok(folder.join("pyes").join("key"))
}

pub fn key_of(environment: Option<&str>, stored: Option<&str>) -> Result<String, String> {
    [environment, stored]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|key| !key.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "no API key: set TYPESAFE_API_KEY or run `pyes auth`".to_string())
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

    file.write_all(key.as_bytes()).map_err(failed)
}

#[cfg(test)]
#[path = "key_of.test.rs"]
mod tests;
