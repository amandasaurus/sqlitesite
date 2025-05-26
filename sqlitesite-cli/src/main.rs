#![allow(warnings)]
use anyhow::Result;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use libsqlitesite::{PageResponse, SqliteSite};
use std::fs::File;
use std::io::BufWriter;
use std::io::{Read, Write};
use std::iter;
use std::path::PathBuf;
use walkdir::WalkDir;

mod import_dir;

#[derive(Parser)]
struct Args {
    db_path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Create {},
    Ls {},
    Summary {},
    Insert {
        url: String,
        content: String,
    },

    Get {
        url: String,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Print all URLs which match this SQL pattern.
    SearchURL {
        pattern: String,
    },

    /// Print all URLs where the raw JSON-encoded HTTP header matches this string
    SearchHeaders {
        pattern: String,
    },
    ImportDir {
        #[arg(long)]
        drop_trailing_index_html: bool,

        #[arg(long)]
        zstd_dictionary: Vec<String>,
        dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Commands::Create {} => {
            let site = SqliteSite::create(args.db_path)?;
        }

        Commands::Ls {} => {
            let site = SqliteSite::open(args.db_path)?;
            for url in site.urls(None)? {
                println!("{}", url);
            }
        }

        Commands::Get { url, output } => {
            let mut site = SqliteSite::open(args.db_path)?;
            let resp = site.get_url(&url).unwrap();
            match resp {
                PageResponse::http4xx => println!("No such URL {}", url),
                PageResponse::http3xx(_) => {
                    todo!()
                }
                PageResponse::http200(_headers, bytes) => match output {
                    None => {
                        println!("{}", String::from_utf8(bytes.into_vec())?);
                    }
                    Some(output) => {
                        let mut f = BufWriter::new(File::create(output)?);
                        write!(f, "{}", String::from_utf8(bytes.into_vec())?);
                    }
                },
            }
        }
        Commands::SearchURL { pattern } => {
            let mut site = SqliteSite::open(&args.db_path)?;
            let result = site.search_urls(&pattern).unwrap();
            if result.is_empty() {
                println!(
                    "0 urls in {} matched sqlite pattern {}",
                    args.db_path.display(),
                    pattern
                );
            } else {
                println!(
                    "{} url(s) in {} matched sqlite pattern {}:",
                    result.len(),
                    args.db_path.display(),
                    pattern
                );
                for url in result {
                    println!("{}", url);
                }
            }
        }
        Commands::SearchHeaders { pattern } => {
            let mut site = SqliteSite::open(&args.db_path)?;
            let result = site.search_headers(&pattern).unwrap();
            for url in result.into_iter() {
                println!("{}", url);
            }
        }
        Commands::Insert { url, content } => {
            let mut site = SqliteSite::open_or_create(args.db_path)?;
            site.set_url(&url, None, None, content.as_bytes())?;
        }
        Commands::Summary {} => {
            let mut site = SqliteSite::open(&args.db_path)?;
            println!(
                "{} has {} url(s).",
                args.db_path.display(),
                site.num_urls()?
            );
            println!(
                "URLs:\n{}",
                site.urls(20)?
                    .into_iter()
                    .map(|u| format!(
                        " • {} {} B",
                        u,
                        site.get_url(u).unwrap().into_200_contents().unwrap().len()
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        Commands::ImportDir {
            drop_trailing_index_html,
            zstd_dictionary,
            dir,
        } => {
            import_dir::import_dir(
                &args.db_path,
                drop_trailing_index_html,
                zstd_dictionary,
                &dir,
            )?;
        }
    }

    Ok(())
}
