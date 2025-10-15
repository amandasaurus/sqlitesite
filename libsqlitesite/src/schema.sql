CREATE TABLE urls (
	url TEXT UNIQUE NOT NULL,
	zstd_dictionary INTEGER DEFAULT NULL,
	http_response_headers INTEGER DEFAULT NULL,
	contents BLOB
);

CREATE TABLE zstd_dictionaries (
	id INTEGER PRIMARY KEY,
	bytes BLOB NOT NULL UNIQUE
);

CREATE TABLE http_response_headers (
	id INTEGER PRIMARY KEY,
	headers_json BLOB NOT NULL UNIQUE
);

CREATE TABLE metadata (
	name TEXT PRIMARY KEY,
	value TEXT NOT NULL
);
