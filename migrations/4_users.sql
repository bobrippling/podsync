-- TODO
CREATE TABLE IF NOT EXISTS users (
	username TEXT NOT NULL PRIMARY KEY,
	pwhash TEXT NOT NULL,
	session_id TEXT -- null == logged out
);
