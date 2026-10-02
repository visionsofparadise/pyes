use super::*;

fn with_base_url(base_url: Option<&str>) -> Environment {
    Environment {
        api_key: None,
        base_url: base_url.map(str::to_string),
        appdata: None,
        xdg_config_home: None,
        home: None,
        not_unicode: Vec::new(),
    }
}

#[test]
fn a_blank_base_url_counts_as_unset() {
    assert_eq!(base_url_of(&with_base_url(Some(" "))), Ok(BASE_URL));
    assert_eq!(base_url_of(&with_base_url(None)), Ok(BASE_URL));
    assert_eq!(
        base_url_of(&with_base_url(Some("http://127.0.0.1:1"))),
        Ok("http://127.0.0.1:1")
    );
}

#[test]
fn a_base_url_that_is_not_unicode_raises() {
    let mut environment = with_base_url(None);

    environment.not_unicode.push("TYPESAFE_BASE_URL");

    assert_eq!(
        base_url_of(&environment),
        Err("TYPESAFE_BASE_URL is not valid Unicode".to_string())
    );
}

#[test]
fn variables_that_are_not_unicode_are_listed_by_name() {
    let environment = environment_of(|name| match name {
        "TYPESAFE_API_KEY" | "HOME" => Err(VarError::NotUnicode(OsString::from("x"))),
        "APPDATA" => Ok("C:/data".to_string()),
        _ => Err(VarError::NotPresent),
    });

    assert_eq!(environment.not_unicode, vec!["TYPESAFE_API_KEY", "HOME"]);
    assert_eq!(
        (
            environment.api_key,
            environment.appdata.as_deref(),
            environment.home
        ),
        (None, Some("C:/data"), None)
    );
}
