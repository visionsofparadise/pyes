use super::*;

fn environment_of(base_url: Option<&str>) -> Environment {
    Environment {
        api_key: None,
        base_url: base_url.map(str::to_string),
        appdata: None,
        xdg_config_home: None,
        home: None,
    }
}

#[test]
fn a_blank_base_url_counts_as_unset() {
    assert_eq!(base_url_of(&environment_of(Some(" "))), BASE_URL);
    assert_eq!(base_url_of(&environment_of(None)), BASE_URL);
    assert_eq!(
        base_url_of(&environment_of(Some("http://127.0.0.1:1"))),
        "http://127.0.0.1:1"
    );
}
