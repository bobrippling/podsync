CREATE TABLE IF NOT EXISTS devices (
	id TEXT NOT NULL,
	username TEXT NOT NULL,

	caption TEXT,
	type TEXT NOT NULL, -- defaulted in code

	UNIQUE(id, username)
);
