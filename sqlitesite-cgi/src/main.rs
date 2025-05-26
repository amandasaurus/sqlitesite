#![allow(warnings)]
use libsqlitesite::{PageResponse, SqliteSite};
use std::env;
use urlencoding;
extern crate cgi;
use anyhow::{Context, Result};

cgi::cgi_try_main! { |request: cgi::Request| -> Result<cgi::Response> {
    let mut db_path = env::current_dir().unwrap();
    db_path.push("site.sqlitesite");
    anyhow::ensure!(db_path.exists(), "Cannot find {:?}", db_path);
    anyhow::ensure!(db_path.is_file(), "DB path {:?} is not a file", db_path);
    let site = SqliteSite::open(&db_path)?;
    let mut desired_url = request.uri().to_string();
    if !desired_url.starts_with("/") {
        desired_url = format!("/{}", desired_url);
    }
    let desired_url = urlencoding::decode(&desired_url).unwrap();
    //return Ok(cgi::text_response(200, format!("Here is the desired URL:\n{}\n{:?}\n", desired_url, desired_url)));
    let url_contents = site.get_c14n_url(&desired_url);

    match url_contents.unwrap() {
        PageResponse::http4xx => Ok(cgi::empty_404()),
        PageResponse::http3xx(new_url) => {
            Ok(cgi::redirect_permanent(new_url))
        },
        PageResponse::http200(headers, byte_contents) => {
            let mut response = cgi::binary_response(200, None, byte_contents.into_vec());
            if let Some(wanted_headers) = headers {
                let resp_headers = response.headers_mut();
                for (key, value) in wanted_headers.into_iter() {
                    resp_headers.insert(cgi::http::header::HeaderName::from_bytes(key.as_bytes()).unwrap(), value.parse().unwrap());
                }
            }
            Ok(response)
        },
    }
} }
