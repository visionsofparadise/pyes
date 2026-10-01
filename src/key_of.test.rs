use super::*;
use std::path::Path;

#[test]
fn the_environment_key_wins() {
    assert_eq!(
        key_of(Some(" env\n"), Some("stored")),
        Ok("env".to_string())
    );
}

#[test]
fn an_empty_environment_key_falls_through_to_the_stored_key() {
    assert_eq!(key_of(Some(""), Some("stored\n")), Ok("stored".to_string()));
    assert_eq!(key_of(Some("  "), Some("stored")), Ok("stored".to_string()));
    assert_eq!(key_of(None, Some("stored")), Ok("stored".to_string()));
}

#[test]
fn no_key_names_both_sources() {
    let missing = Err("no API key: set TYPESAFE_API_KEY or run `pyes auth`".to_string());

    assert_eq!(key_of(None, None), missing);
    assert_eq!(key_of(Some(""), Some("\n")), missing);
}

#[test]
fn windows_reads_appdata() {
    assert_eq!(
        key_path_of(true, Some("C:/data"), Some("/xdg"), Some("/home")),
        Ok(Path::new("C:/data").join("pyes").join("key"))
    );
    assert_eq!(
        key_path_of(true, None, Some("/xdg"), Some("/home")),
        Err("APPDATA is not set".to_string())
    );
}

#[test]
fn elsewhere_xdg_config_home_wins_over_home() {
    assert_eq!(
        key_path_of(false, Some("C:/data"), Some("/xdg"), Some("/home")),
        Ok(Path::new("/xdg").join("pyes").join("key"))
    );
}

#[test]
fn elsewhere_home_config_is_the_fallback() {
    assert_eq!(
        key_path_of(false, None, Some(""), Some("/home")),
        Ok(Path::new("/home").join(".config").join("pyes").join("key"))
    );
    assert_eq!(
        key_path_of(false, None, None, None),
        Err("neither XDG_CONFIG_HOME nor HOME is set".to_string())
    );
}
