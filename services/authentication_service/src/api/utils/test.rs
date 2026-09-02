use super::*;

#[test]
fn token_cookies_follow_the_environment_policy() {
    for (environment, domain, secure, same_site, access_name, refresh_name) in [
        (
            Environment::Local,
            None,
            false,
            SameSite::Lax,
            format!("local-{MACRO_ACCESS_TOKEN_COOKIE}"),
            format!("local-{MACRO_REFRESH_TOKEN_COOKIE}"),
        ),
        (
            Environment::Develop,
            Some("macro.com"),
            true,
            SameSite::None,
            format!("dev-{MACRO_ACCESS_TOKEN_COOKIE}"),
            format!("dev-{MACRO_REFRESH_TOKEN_COOKIE}"),
        ),
        (
            Environment::Production,
            Some("macro.com"),
            true,
            SameSite::Strict,
            MACRO_ACCESS_TOKEN_COOKIE.to_string(),
            MACRO_REFRESH_TOKEN_COOKIE.to_string(),
        ),
    ] {
        assert_cookie(
            create_token_cookie(environment, MACRO_ACCESS_TOKEN_COOKIE, "access-token"),
            &access_name,
            "access-token",
            domain,
            secure,
            same_site,
        );
        assert_cookie(
            create_token_cookie(environment, MACRO_REFRESH_TOKEN_COOKIE, "refresh-token"),
            &refresh_name,
            "refresh-token",
            domain,
            secure,
            same_site,
        );
    }
}

fn assert_cookie(
    cookie: Cookie<'_>,
    name: &str,
    value: &str,
    domain: Option<&str>,
    secure: bool,
    same_site: SameSite,
) {
    assert_eq!(cookie.name(), name);
    assert_eq!(cookie.value(), value);
    assert_eq!(cookie.domain(), domain);
    assert_eq!(cookie.secure(), Some(secure));
    assert_eq!(cookie.same_site(), Some(same_site));
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(cookie.path(), Some("/"));
    assert!(cookie.expires().is_some());
}
