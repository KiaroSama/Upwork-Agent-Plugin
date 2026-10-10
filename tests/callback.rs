use upwork_connector::callback;
#[test]
fn complete_manual_authorization_url_stays_exact() {
    let url = "https://example.com/authorize?state=example&code_challenge=abc%2B123";
    assert_eq!(callback::authorization_url(url).unwrap().as_str(), url);
}
#[test]
fn unsafe_browser_links_rejected() {
    for url in [
        "http://example.com/a",
        "https://u:p@example.com/a",
        "javascript:alert(1)",
        "https://example.com/a\nInjected",
    ] {
        assert!(callback::authorization_url(url).is_err());
    }
}
#[test]
fn callback_method_path_empty_duplicate_and_utf8_rejected() {
    let expected = url::Url::parse("http://localhost:23037/oauth/callback").unwrap();
    for request in [
        "GET /wrong?code=a&state=b HTTP/1.1\r\n\r\n",
        "POST /oauth/callback?code=a&state=b HTTP/1.1\r\n\r\n",
        "GET /oauth/callback?code=&state=b HTTP/1.1\r\n\r\n",
        "GET /oauth/callback?code=a&state=b&state=c HTTP/1.1\r\n\r\n",
        "GET /oauth/callback?code=a&state=b&error=denied HTTP/1.1\r\n\r\n",
    ] {
        assert!(callback::parameters(request.as_bytes(), &expected).is_err());
    }
    assert!(callback::parameters(&[255], &expected).is_err());
}
#[test]
fn callback_decodes_parameters_without_logging() {
    let expected = url::Url::parse("http://localhost:23037/oauth/callback").unwrap();
    let p=callback::parameters(b"GET /oauth/callback?code=a%2Bb&state=c&iss=https%3A%2F%2Fissuer.example HTTP/1.1\r\nHost: localhost\r\n\r\n",&expected).unwrap();
    assert_eq!(p["code"], "a+b");
    assert_eq!(p["iss"], "https://issuer.example");
}
