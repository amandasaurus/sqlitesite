use super::*;

#[test]
fn c14n_url0() {
    assert_eq!(c14n_url("/"), "/");
    assert_eq!(c14n_url("//"), "/");
    assert_eq!(c14n_url(""), "/");
    assert_eq!(c14n_url("/hello"), "/hello");
    assert_eq!(c14n_url("/hello/world"), "/hello/world");
    assert_eq!(c14n_url("//hello/world"), "/hello/world");
    assert_eq!(c14n_url("//////hello/world"), "/hello/world");
    assert_eq!(c14n_url("/hello//world"), "/hello/world");
    assert_eq!(c14n_url("//hello///world"), "/hello/world");
    assert_eq!(c14n_url("hello"), "/hello");
    assert_eq!(c14n_url("hello/world"), "/hello/world");
}

#[test]
fn c14n_url_w_slash1() {
    assert_eq!(c14n_url_w_slash("/"), "/");
    assert_eq!(c14n_url_w_slash("/hello/world"), "/hello/world/");
    assert_eq!(c14n_url_w_slash("/hello/world/"), "/hello/world/");
    assert_eq!(c14n_url_w_slash("/test.html"), "/test.html");
    assert_eq!(c14n_url_w_slash("/foo/bar.html"), "/foo/bar.html");
    assert_eq!(c14n_url_w_slash("/foo/bar"), "/foo/bar/");
    assert_eq!(c14n_url_w_slash("/foo/bar/"), "/foo/bar/");
    assert_eq!(c14n_url_w_slash("/foo/bar?q=hello"), "/foo/bar?q=hello");
    assert_eq!(c14n_url_w_slash("/foo/bar#hello"), "/foo/bar#hello");
}

#[test]
fn metadata() {
    let mut db = SqliteSite::create_in_memory().unwrap();
    assert_eq!(db.metadata("hello").unwrap(), None);
    db.set_metadata("hello", "world").unwrap();
    assert_eq!(db.metadata("foo").unwrap(), None);
    assert_eq!(db.metadata("hello").unwrap(), Some("world".to_string()));
}
