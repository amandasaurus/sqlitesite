# sqlitesite - Store static website in a SQLite database

If you have a static website with many tiny HTML files, which are very similar, it can take a lot of space, and the file i/o overhead can make operations time consuming.
This tool stores all HTML page (& other static assets) in a simple SQLite database. There is a provided CGI programme, and simple http server, to serve from this format

## Components

### `libsqlitesite`

Core, Rust library.

### `sqlitesite-cli`

Command line tool for interacting with a sqlitesite file. Simple CRUD options possible.

### `sqlitesite-cgi`

CGI programme for serving web requests from a SQLite database

### `sqlitesite-web`

Simple, dev tool to serve up files from that sqlitesite.

## Projects using this

* WaterwayMap River Database

## Copyright

Copyright MIT or Apache-2.0, 2024 Amanda McCann <amanda@technomancy.org>


