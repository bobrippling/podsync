-- TODO: unique?

CREATE TABLE IF NOT EXISTS devices (
	id TEXT NOT NULL,
	caption TEXT NOT NULL,
	username TEXT NOT NULL,
	type TEXT NOT NULL,

	UNIQUE(id)
);
