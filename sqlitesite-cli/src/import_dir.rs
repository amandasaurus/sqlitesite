use anyhow::Result;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use libsqlitesite::SqliteSite;
use std::fs::File;
use std::io::{Read, Write};
use std::iter;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub(crate) fn import_dir(
    db_path: &Path,
    drop_trailing_index_html: bool,
    zstd_dictionary: Vec<String>,
    dir: &Path,
) -> Result<()> {
    anyhow::ensure!(dir.is_dir());
    let mut site = SqliteSite::open_or_create(&db_path)?;

    let mut zstd_dictionaries = zstd_dictionary
        .into_iter()
        .map(|raw| {
            let mut parts = raw.splitn(2, ":");
            let file_ext = parts.next().unwrap().to_string();
            let zstd_dictionary_bytes = std::fs::read(parts.next().unwrap())
                .unwrap()
                .into_boxed_slice();
            let zstd_dictionary_id = site
                .get_or_create_zstd_dictionary(&zstd_dictionary_bytes)
                .unwrap();
            let zstd_compressor =
                zstd::bulk::Compressor::with_dictionary(3, &zstd_dictionary_bytes).unwrap();
            (file_ext, zstd_compressor, zstd_dictionary_id)
        })
        .collect::<Vec<(String, _, u32)>>();
    println!(
        "Importing pages from {} into {}",
        dir.display(),
        db_path.display()
    );
    let files_added = ProgressBar::new_spinner();
    let mut total_files_added = 0;
    files_added.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {human_pos} Adding pages. {per_sec} pages/sec",
        )
        .unwrap(),
    );
    let mut bulk_inserter = site.start_bulk()?;
    let mut contents = Vec::new();
    for entry in WalkDir::new(&dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry
            .path()
            .strip_prefix(&dir)
            .unwrap()
            .display()
            .to_string();
        let mut file = File::open(entry.path()).unwrap();
        contents.truncate(0);
        file.read_to_end(&mut contents).unwrap();

        let mut zstd_dictionary_id_opt = None;
        if let Some(file_ext) = entry.path().extension().and_then(|e| e.to_str()) {
            assert!(zstd_dictionaries.iter().filter(|x| x.0 == file_ext).count() < 2);
            if let Some((_pattern, ref mut zstd_compressor, zstd_dictionary_id)) = zstd_dictionaries
                .iter_mut()
                .filter(|x| x.0 == file_ext)
                .take(1)
                .next()
            {
                let mut new_contents = zstd_compressor.compress(&contents)?;
                zstd_dictionary_id_opt = Some(*zstd_dictionary_id);
                std::mem::replace(&mut contents, new_contents);
            }
        }

        let mut url = format!("/{}", path);
        if drop_trailing_index_html {
            if let Some(prefix) = url.strip_suffix("/index.html") {
                url = prefix.to_string();
            }
            if url == "" {
                url = "/".to_string();
            }
        }
        files_added.inc(1);
        total_files_added += 1;

        bulk_inserter.add_unique_url(url, zstd_dictionary_id_opt, None, &contents)?;
    }

    bulk_inserter.finish()?;

    println!(
        "{} pages imported into {}",
        total_files_added,
        db_path.display()
    );
    Ok(())
}
