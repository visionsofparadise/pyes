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
fn a_base_url_is_used_trimmed() {
    assert_eq!(
        base_url_of(&with_base_url(Some(" http://127.0.0.1:1\n"))),
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

#[test]
fn each_variable_is_read_by_its_own_name() {
    let environment = environment_of(|name| Ok(name.to_string()));

    assert_eq!(
        [
            environment.api_key,
            environment.base_url,
            environment.appdata,
            environment.xdg_config_home,
            environment.home,
        ],
        [
            "TYPESAFE_API_KEY",
            "TYPESAFE_BASE_URL",
            "APPDATA",
            "XDG_CONFIG_HOME",
            "HOME",
        ]
        .map(|name| Some(name.to_string()))
    );
}

fn run_of(arguments: &[&str]) -> (String, String, i32) {
    let arguments = std::iter::once("pyes")
        .chain(arguments.iter().copied())
        .map(OsString::from)
        .collect();
    let mut stdin: &[u8] = b"";
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run(
        arguments,
        &with_base_url(None),
        &mut Streams {
            stdin: &mut stdin,
            stdout: &mut stdout,
            stderr: &mut stderr,
        },
    );

    (
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
        code,
    )
}

#[test]
fn help_prints_to_stdout_and_exits_zero() {
    let (stdout, stderr, code) = run_of(&["--help"]);

    assert!(stdout.contains("Usage: pyes"));
    assert_eq!((stderr, code), (String::new(), SUCCESS));
}

#[test]
fn version_prints_to_stdout_and_exits_zero() {
    assert_eq!(
        run_of(&["--version"]),
        (
            format!("pyes {}\n", env!("CARGO_PKG_VERSION")),
            String::new(),
            SUCCESS
        )
    );
}

#[test]
fn a_clap_error_prints_its_first_line_and_exits_two() {
    assert_eq!(
        run_of(&["--bogus", "Is this a?"]),
        (
            String::new(),
            "pyes: unexpected argument '--bogus' found\n".to_string(),
            FAILURE
        )
    );
}

#[test]
fn a_panic_payload_becomes_an_internal_error_message() {
    let payloads: [Box<dyn Any + Send>; 3] = [
        Box::new("static text"),
        Box::new("owned text".to_string()),
        Box::new(42),
    ];
    let messages: Vec<String> = payloads
        .iter()
        .map(|payload| panic_message_of(&**payload))
        .collect();

    assert_eq!(
        messages,
        [
            "internal error: static text",
            "internal error: owned text",
            "internal error: a panic ended the run",
        ]
    );
}
