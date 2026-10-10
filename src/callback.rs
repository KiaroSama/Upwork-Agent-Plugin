use crate::error::{Error, Result};
use std::collections::HashMap;
use url::Url;

pub fn authorization_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| Error::Safe("Unsafe authorization link refused."))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || raw.chars().any(char::is_control)
    {
        return Err(Error::Safe("Unsafe authorization link refused."));
    }
    Ok(url)
}
pub fn parameters(request: &[u8], expected: &Url) -> Result<HashMap<String, String>> {
    let text = std::str::from_utf8(request).map_err(|_| Error::Safe("Invalid local callback."))?;
    if !text.ends_with("\r\n\r\n") {
        return Err(Error::Safe("Incomplete local callback."));
    }
    let line = text
        .lines()
        .next()
        .ok_or(Error::Safe("Invalid local callback."))?;
    let path = line
        .strip_prefix("GET ")
        .and_then(|s| s.split_once(' '))
        .map(|(p, _)| p)
        .ok_or(Error::Safe("Invalid callback method."))?;
    if !path.starts_with('/') || path.starts_with("//") {
        return Err(Error::Safe("Invalid callback path."));
    }
    let callback = Url::parse(&format!("http://localhost{path}"))
        .map_err(|_| Error::Safe("Invalid local callback."))?;
    if callback.path() != expected.path() {
        return Err(Error::Safe("Unexpected callback path."));
    }
    let mut params = HashMap::new();
    for (key, value) in callback.query_pairs() {
        if params
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(Error::Safe("Duplicate callback parameter."));
        }
    }
    if params.get("code").is_none_or(String::is_empty)
        || params.get("state").is_none_or(String::is_empty)
        || params.contains_key("error")
    {
        return Err(Error::Safe("Authorization callback rejected."));
    }
    Ok(params)
}
