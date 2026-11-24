//! The parser behind the sandbox's `URL` class: the WHATWG URL Standard as the `url` crate
//! implements it, through `url::quirks`, the crate's own mapping of the JavaScript API's getters
//! and setters. js/workers-globals.js keeps a URL's parts as the JSON this returns.

use serde_json::json;
use url::{quirks, Url};

fn parts(u: &Url) -> String {
    json!({
        "href": quirks::href(u),
        "origin": quirks::origin(u),
        "protocol": quirks::protocol(u),
        "username": quirks::username(u),
        "password": quirks::password(u),
        "host": quirks::host(u),
        "hostname": quirks::hostname(u),
        "port": quirks::port(u),
        "pathname": quirks::pathname(u),
        "search": quirks::search(u),
        "hash": quirks::hash(u),
    })
    .to_string()
}

/// `op("parse", input, base, _)`: the URL's parts, or None when it does not parse.
/// `op("set", href, part, value)`: the parts after `url.<part> = value`; a value the setter
/// ignores leaves them as they were, and None means `href` was set to something that is no URL.
pub(crate) fn op(op: String, a: String, b: Option<String>, c: Option<String>) -> Option<String> {
    match op.as_str() {
        "parse" => {
            let base = match b {
                Some(base) => Some(Url::parse(&base).ok()?),
                None => None,
            };
            Url::options().base_url(base.as_ref()).parse(&a).ok().map(|u| parts(&u))
        }
        "set" => {
            let mut u = Url::parse(&a).ok()?;
            let value = c.unwrap_or_default();
            match b.as_deref()? {
                "href" => quirks::set_href(&mut u, &value).ok()?,
                "protocol" => drop(quirks::set_protocol(&mut u, &value)),
                "username" => drop(quirks::set_username(&mut u, &value)),
                "password" => drop(quirks::set_password(&mut u, &value)),
                "host" => drop(quirks::set_host(&mut u, &value)),
                "hostname" => drop(quirks::set_hostname(&mut u, &value)),
                "port" => drop(quirks::set_port(&mut u, &value)),
                "pathname" => quirks::set_pathname(&mut u, &value),
                "search" => quirks::set_search(&mut u, &value),
                "hash" => quirks::set_hash(&mut u, &value),
                _ => {}
            }
            Some(parts(&u))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::op;
    use serde_json::Value;

    fn parse(input: &str, base: Option<&str>) -> Option<Value> {
        op("parse".into(), input.into(), base.map(str::to_string), None).map(|s| serde_json::from_str(&s).unwrap())
    }

    #[test]
    fn urls_parse_as_the_standard_says() {
        let u = parse("https://user:pw@Example.com:8443/a/b%20c/../d?x=1&y=two#frag", None).unwrap();
        assert_eq!(u["href"], "https://user:pw@example.com:8443/a/d?x=1&y=two#frag");
        assert_eq!((u["origin"].as_str(), u["host"].as_str(), u["port"].as_str()), (Some("https://example.com:8443"), Some("example.com:8443"), Some("8443")));
        assert_eq!(parse("../c?d=1", Some("https://example.com/a/b")).unwrap()["href"], "https://example.com/c?d=1");
        assert_eq!(parse("http://0x7f.1/", None).unwrap()["hostname"], "127.0.0.1");
        assert_eq!(parse("https://bücher.example/", None).unwrap()["hostname"], "xn--bcher-kva.example");
        assert!(parse("not a url", None).is_none() && parse("/relative", None).is_none() && parse("x", Some("nope")).is_none());
        let set = |href: &str, part: &str, value: &str| -> Value {
            serde_json::from_str(&op("set".into(), href.into(), Some(part.into()), Some(value.into())).unwrap()).unwrap()
        };
        assert_eq!(set("https://example.com/a?x=1", "pathname", "/b c")["href"], "https://example.com/b%20c?x=1");
        assert_eq!(set("https://example.com/", "search", "a=1&b=2")["search"], "?a=1&b=2");
        assert_eq!(set("https://example.com/", "port", "nope")["href"], "https://example.com/");
        assert!(op("set".into(), "https://example.com/".into(), Some("href".into()), Some("nope".into())).is_none());
    }
}
