#![allow(warnings)]
use clap::{Parser, Subcommand};
use libsqlitesite::{PageResponse, SqliteSite};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Parser)]
struct Args {
    db_path: PathBuf,
}

use rouille::Request;
use rouille::Response;

fn main() {
    let args = Args::parse();
    let db_path = args.db_path.clone();
    assert!(db_path.exists());
    assert!(db_path.is_file());

    println!("Listening on port 8000");

    rouille::start_server("0.0.0.0:8000", move |request| {
        let site = SqliteSite::open(&db_path).unwrap();
        println!("{}", request.url());
        let resp = match site.get_c14n_url(&request.url()) {
            Ok(x) => x,
            Err(err) => {
                eprintln!(
                    "Error fetching URL {} from DB ({}), error: {:?}",
                    request.url(),
                    db_path.display(),
                    err
                );
                return Response::text("error").with_status_code(500);
            }
        };
        match resp {
            PageResponse::http4xx => Response::empty_404(),
            PageResponse::http3xx(new_url) => Response::redirect_301(new_url),
            PageResponse::http200(headers, bytes) => {
                let mut resp = Response::from_data("", bytes);
                for (key, value) in headers.into_iter().flatten() {
                    resp.headers.push((key.into(), value.into()));
                }
                resp
            }
        }
    });
}
