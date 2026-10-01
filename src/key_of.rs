use std::path::PathBuf;

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

#[cfg(test)]
#[path = "key_of.test.rs"]
mod tests;
