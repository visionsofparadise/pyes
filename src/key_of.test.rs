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
        Err("XDG_CONFIG_HOME is not set and HOME is not set".to_string())
    );
}

#[test]
fn a_leading_byte_order_mark_is_stripped() {
    assert_eq!(bare_key_of("\u{feff} key\r\n"), "key");
    assert_eq!(
        key_of(None, Some("\u{feff}stored")),
        Ok("stored".to_string())
    );
}

#[test]
fn a_key_outside_visible_ascii_is_refused() {
    let refused = Err("the API key contains characters an HTTP header cannot carry".to_string());

    assert_eq!(key_of(Some("a b"), None), refused);
    assert_eq!(key_of(None, Some("ké")), refused);
    assert_eq!(header_key_of("a\u{7f}"), refused);
    assert_eq!(header_key_of("!~"), Ok("!~".to_string()));
}

#[test]
fn elsewhere_a_relative_xdg_config_home_is_ignored() {
    assert_eq!(
        key_path_of(false, None, Some("relative/xdg"), Some("/home")),
        Ok(Path::new("/home").join(".config").join("pyes").join("key"))
    );
}

#[test]
fn windows_refuses_a_relative_appdata() {
    for appdata in ["data", r"C:data", r"\data"] {
        assert_eq!(
            key_path_of(true, Some(appdata), Some("/xdg"), Some("/home")),
            Err("APPDATA is relative".to_string())
        );
    }

    for appdata in [r"C:\data", r"\\server\share"] {
        assert_eq!(
            key_path_of(true, Some(appdata), None, None),
            Ok(Path::new(appdata).join("pyes").join("key"))
        );
    }
}

#[test]
fn elsewhere_a_relative_home_counts_as_unset_and_the_error_names_each_cause() {
    assert_eq!(
        key_path_of(false, None, Some("relative/xdg"), Some("relative/home")),
        Err("XDG_CONFIG_HOME is relative and HOME is relative".to_string())
    );
    assert_eq!(
        key_path_of(false, None, Some("relative/xdg"), None),
        Err("XDG_CONFIG_HOME is relative and HOME is not set".to_string())
    );
    assert_eq!(
        key_path_of(false, None, None, Some("home")),
        Err("XDG_CONFIG_HOME is not set and HOME is relative".to_string())
    );
}
