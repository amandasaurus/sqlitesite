//! Store websites in a a compressed single file database
#![allow(warnings)]
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Transaction};
use std::borrow::Cow;
use std::io::Read;
use std::iter;
use std::path::Path;

#[cfg(test)]
mod tests;

/// A single webpage database
#[derive(Debug)]
pub struct SqliteSite {
    db: Connection,
}

/// The type of HTTP response possible for a URL.
#[derive(PartialEq, Debug)]
pub enum PageResponse {
    http200(Option<Vec<(String, String)>>, Box<[u8]>),
    http3xx(String),
    http4xx,
}

impl PageResponse {
    pub fn into_200_contents(self) -> Option<Box<[u8]>> {
        if let PageResponse::http200(_headers, bytes) = self {
            Some(bytes)
        } else {
            None
        }
    }
}

/// Canonicalise a URL
pub fn c14n_url<'a>(url: impl Into<Cow<'a, str>>) -> Cow<'a, str> {
    let mut url: Cow<'a, str> = url.into();

    while url.contains("//") {
        url = Cow::Owned(url.replace("//", "/"));
    }
    if !url.starts_with("/") {
        url = Cow::Owned(format!("/{}", url));
    }

    url
}

fn url_can_be_slashhed<'a>(url: impl Into<Cow<'a, str>>) -> bool {
    let mut url: Cow<'a, str> = url.into();
    !url.ends_with("/")
        && url
            .rsplit("/")
            .next()
            .map_or(false, |last_part| !last_part.contains(&['.', '?', '#']))
}

/// Canonicalise URLs, and optionally add a trailing slash if appropriate.
/// Useful when adding page
pub fn c14n_url_w_slash<'a>(url: impl Into<Cow<'a, str>>) -> Cow<'a, str> {
    let mut url: Cow<'a, str> = c14n_url(url);

    if url_can_be_slashhed(&*url) {
        url = Cow::Owned(format!("{}/", url));
    }

    url
}

impl SqliteSite {
    fn from_conn(db: Connection) -> Self {
        SqliteSite { db }
    }

    /// Create and return a new SqliteSite for this path.
    /// Error if the page already exists
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        anyhow::ensure!(!path.exists(), "Path {} already  exists", path.display());

        let mut db = Connection::open(path)?;
        db.execute_batch(include_str!("schema.sql"))?;

        Ok(Self::from_conn(db))
    }
    #[cfg(test)]
    pub fn create_in_memory() -> Result<Self> {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch(include_str!("schema.sql"))?;

        Ok(Self::from_conn(db))
    }

    /// Open an existing database file.
    /// Errors if it doesn't exist
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        anyhow::ensure!(path.exists(), "Path {} doesn't exists", path.display());
        anyhow::ensure!(path.is_file(), "Path {} is not a file", path.display());

        let mut db = Connection::open(path)?;

        Ok(Self::from_conn(db))
    }

    /// Open an existing file, or create it if needed
    pub fn open_or_create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.exists() {
            Self::open(path)
        } else {
            Self::create(path)
        }
    }

    /// Number of URLs in this site
    pub fn num_urls(&self) -> Result<usize> {
        Ok(self
            .db
            .query_row("SELECT COUNT(*) FROM urls;", [], |row| row.get(0))?)
    }

    /// Return the ID of the zstd dict with those bytes, creating it if needed.
    /// If a dict already exists with exactly that binary content, return that's id
    pub fn get_or_create_zstd_dictionary(&self, zstd_dictionary_bytes: &[u8]) -> Result<u32> {
        self.db.execute(
            "INSERT OR IGNORE INTO zstd_dictionaries (bytes) VALUES (?1);",
            [zstd_dictionary_bytes],
        )?;
        let zstd_dictionary_id: u32 = self.db.query_row(
            "SELECT id FROM zstd_dictionaries WHERE bytes = ?1;",
            [zstd_dictionary_bytes],
            |row| row.get(0),
        )?;

        Ok(zstd_dictionary_id)
    }

    /// Return the ID to use for those HTTP response headers
    pub fn get_or_create_http_response_headers_id(
        &self,
        mut headers: Vec<(String, String)>,
    ) -> Result<u32> {
        headers.sort();
        let headers_json = serde_json::to_string(&headers)?;

        self.db.execute(
            "INSERT OR IGNORE INTO http_response_headers (headers_json) VALUES (?1);",
            [&headers_json],
        )?;
        let http_headers_id: u32 = self.db.query_row(
            "SELECT id FROM http_response_headers WHERE headers_json = ?1;",
            [&headers_json],
            |row| row.get(0),
        )?;

        Ok(http_headers_id)
    }

    /// For this URL, first canonicalise it, then get the response for it
    pub fn get_c14n_url(&self, url: &str) -> Result<PageResponse> {
        let new_url = c14n_url(url);
        if new_url != url {
            return Ok(PageResponse::http3xx(new_url.to_string()));
        }
        let direct_resp = self.get_url(url)?;
        if direct_resp == PageResponse::http4xx && url_can_be_slashhed(url) {
            return Ok(PageResponse::http3xx(format!("{}/", url)));
        }

        Ok(direct_resp)
    }

    /// Look up exactly this URL in the DB & return the response
    pub fn get_url(&self, url: &str) -> Result<PageResponse> {
        let res: Option<(Option<u32>, Option<u32>, Box<[u8]>)> = self
            .db
            .query_row(
                "SELECT zstd_dictionary, http_response_headers, contents FROM urls WHERE url = ?1;",
                [url],
                |row| {
                    let zstd_dictionary = row.get(0)?;
                    let http_response_headers = row.get(1)?;
                    let content = row.get(2)?;
                    Ok((zstd_dictionary, http_response_headers, content))
                },
            )
            .optional()?;
        if res.is_none() {
            // TODO try to do a redirect
            return Ok(PageResponse::http4xx);
        }

        let (zstd_dictionary, http_response_headers, mut content) = res.unwrap();

        let http_response_headers = if let Some(hdr_id) = http_response_headers {
            let http_response_headers_json: String = self.db.query_row(
                "SELECT headers_json FROM http_response_headers WHERE id = ?1;",
                [hdr_id],
                |row| row.get(0),
            )?;
            let http_response_headers: Vec<(String, String)> =
                serde_json::from_str(&http_response_headers_json)?;

            Some(http_response_headers)
        } else {
            None
        };

        if let Some(dict_id) = zstd_dictionary {
            let zstd_dictionary: Box<[u8]> = self.db.query_row(
                "SELECT bytes FROM zstd_dictionaries WHERE id = ?1;",
                [dict_id],
                |row| row.get(0),
            )?;
            let mut zstd_decoder = zstd::Decoder::with_dictionary(&*content, &zstd_dictionary)?;
            let mut new_content = Vec::new();
            zstd_decoder.read_to_end(&mut new_content)?;
            std::mem::replace(&mut content, new_content.into_boxed_slice());
        }

        Ok(PageResponse::http200(http_response_headers, content))
    }

    /// Canonicalise that url, then add it to the site
    pub fn set_c14n_url<'a>(
        &mut self,
        url: impl Into<Cow<'a, str>>,
        zstd_dictionary: impl Into<Option<u32>>,
        http_headers_id: impl Into<Option<u32>>,
        contents: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.set_url(
            c14n_url_w_slash(url),
            zstd_dictionary,
            http_headers_id,
            contents,
        )
    }

    /// Set the (optionally compressed) contents of a URL to this.
    /// The contents are not compressed here, the zstd_dictionary_id will be taken raw.
    /// The URL is not canonicalised or changed.
    /// If there is already a page for that URL, it is silently overwritten.
    pub fn set_url<'a>(
        &mut self,
        url: impl Into<Cow<'a, str>>,
        zstd_dictionary: impl Into<Option<u32>>,
        http_headers_id: impl Into<Option<u32>>,
        contents: impl AsRef<[u8]>,
    ) -> Result<()> {
        let url: Cow<'a, str> = url.into();
        let url: &str = url.as_ref();
        anyhow::ensure!(url.starts_with("/"));
        let contents: &[u8] = contents.as_ref();
        self.db.execute(
            "INSERT INTO urls (url, zstd_dictionary, http_response_headers, contents) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (url) DO UPDATE SET zstd_dictionary = ?2, http_response_headers = ?3, contents = ?4;",
            (url, zstd_dictionary.into(), http_headers_id.into(), contents),
        )?;
        Ok(())
    }

    pub fn set_bulk(
        &mut self,
        input_data: impl Iterator<Item = (impl AsRef<str>, Option<u32>, impl AsRef<[u8]>)>,
    ) -> Result<()> {
        let txn = self.db.transaction()?;
        let mut stmt = txn.prepare("INSERT INTO urls (url, zstd_dictionary, contents) VALUES (?1, ?2, ?3) ON CONFLICT (url) DO UPDATE SET zstd_dictionary = ?1, contents = ?2;")?;
        for page in input_data {
            assert!(!page.0.as_ref().is_empty());
            assert!(!page.2.as_ref().is_empty());
            stmt.execute((page.0.as_ref(), page.1, page.2.as_ref()))?;
        }
        drop(stmt);
        txn.commit()?;
        Ok(())
    }

    /// Returns a list of all URLs in this p
    pub fn urls(&self, limit: impl Into<Option<usize>>) -> Result<Box<[String]>> {
        let mut res: Vec<String> = Vec::with_capacity(self.num_urls()?);
        let limit = limit
            .into()
            .map_or("".to_string(), |limit| format!(" LIMIT {}", limit));
        let mut stmt = self
            .db
            .prepare(&format!("SELECT url FROM urls ORDER BY url {};", limit))?;
        res.extend(
            stmt.query_map([], |row| -> rusqlite::Result<String> { row.get(0) })?
                .filter_map(Result::ok),
        );

        Ok(res.into_boxed_slice())
    }

    pub fn search_urls(&self, url_pattern: impl AsRef<str>) -> Result<Box<[String]>> {
        let url_pattern: &str = url_pattern.as_ref();
        let mut res: Vec<String> = Vec::new();
        let mut stmt = self
            .db
            .prepare("SELECT url FROM urls WHERE url LIKE ?1 ORDER BY url;")?;
        res.extend(
            stmt.query_map([url_pattern], |row| -> rusqlite::Result<String> {
                row.get(0)
            })?
            .filter_map(Result::ok),
        );

        Ok(res.into_boxed_slice())
    }

    pub fn search_headers(&self, pattern: impl AsRef<str>) -> Result<Box<[String]>> {
        let pattern: &str = pattern.as_ref();
        let mut res: Vec<String> = Vec::new();

        let mut stmt = self
            .db
            .prepare("select url from urls where http_response_headers IN (select id from http_response_headers where headers_json LIKE ?1) order by url;")?;

        res.extend(
            stmt.query_map([pattern], |row| -> rusqlite::Result<String> { row.get(0) })?
                .filter_map(Result::ok),
        );

        Ok(res.into_boxed_slice())
    }

    /// Start doing a bulk addition of pages
    pub fn start_bulk(&mut self) -> Result<BulkSqliteSiteAdder> {
        BulkSqliteSiteAdder::from_site(self)
    }

    /// Gets a metadata value.
    /// Option if this value doesn't exist.
    pub fn metadata(&self, name: impl AsRef<str>) -> Result<Option<String>> {
        let value: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM metadata WHERE name = ?1;",
                [&name.as_ref()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }
    /// Sets a metadata value
    /// Sets a metadata value
    pub fn set_metadata(&self, name: impl AsRef<str>, value: impl AsRef<str>) -> Result<()> {
        let name: &str = name.as_ref();
        let value: &str = value.as_ref();
        self.db.execute(
            "INSERT INTO metadata (name, value) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET value = excluded.value;",
            [name, value],
        )?;
        Ok(())
    }

    pub fn contents_for_404(&self) -> Result<Option<String>> {
        if self
            .metadata("send_content_for_404")?
            .map_or(false, |v| v == "true")
        {
            let content = self.metadata("content_for_404")?.ok_or(anyhow::anyhow!(
                "content_for_404 is not set, while send_content_for_404 is set to true"
            ))?;
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }
    pub fn set_contents_for_404(&mut self, content: impl AsRef<str>) -> Result<()> {
        let content: &str = content.as_ref();
        self.set_metadata("content_for_404", content)?;
        self.set_metadata("send_content_for_404", "true")?;
        Ok(())
    }
    pub fn set_content_404_sending(&mut self, enabled: bool) -> Result<()> {
        if enabled {
            anyhow::ensure!(
                self.metadata("content_for_404")?.is_some(),
                "Cannot enable special 404 handling , if you have not already set the contents for the 404 page"
            );
        }

        self.set_metadata("content_for_404", if enabled { "true" } else { "false" })?;
        Ok(())
    }
    pub fn enable_404_content(&mut self) -> Result<()> {
        self.set_content_404_sending(true)
    }
    pub fn disable_404_content(&mut self) -> Result<()> {
        self.set_content_404_sending(false)
    }
}

/// Helper struct to add many pages quicker by using prepared queries and transactions
pub struct BulkSqliteSiteAdder<'a> {
    txn: rusqlite::Transaction<'a>,
}

impl<'a> BulkSqliteSiteAdder<'a> {
    pub fn from_site(site: &'a mut SqliteSite) -> Result<Self> {
        let txn = site.db.transaction()?;

        // cache this statment (maybe?)
        txn.prepare_cached("INSERT INTO urls (url, zstd_dictionary, http_response_headers, contents) VALUES (?1, ?2, ?3, ?4);")?;

        Ok(BulkSqliteSiteAdder { txn })
    }
    pub fn url_exists(&mut self, url: impl AsRef<str>) -> Result<bool> {
        let url: &str = url.as_ref();
        let url = c14n_url_w_slash(url);
        let mut stmt = self
            .txn
            .prepare_cached("SELECT COUNT(*) FROM urls WHERE url = ?1;")?;
        let res: u64 = stmt.query_row([url.as_ref()], |row| Ok(row.get(0)?))?;

        Ok(res > 0)
    }

    pub fn add_unique_url(
        &mut self,
        url: impl AsRef<str>,
        zstd_dictionary: impl Into<Option<u32>>,
        http_headers_id: impl Into<Option<u32>>,
        contents: impl AsRef<[u8]>,
    ) -> Result<()> {
        let url: &str = url.as_ref();
        let url = c14n_url_w_slash(url);
        let zstd_dictionary: Option<u32> = zstd_dictionary.into();
        let http_headers_id: Option<u32> = http_headers_id.into();
        let contents = contents.as_ref();

        anyhow::ensure!(!contents.is_empty());

        let mut stmt = self.txn.prepare_cached("INSERT INTO urls (url, zstd_dictionary, http_response_headers, contents) VALUES (?1, ?2, ?3, ?4);")?;
        stmt.execute((url, zstd_dictionary, http_headers_id, contents))?;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        let BulkSqliteSiteAdder { txn } = self;
        txn.flush_prepared_statement_cache();
        txn.commit()?;
        Ok(())
    }
}
