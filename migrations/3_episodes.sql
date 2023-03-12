CREATE TABLE IF NOT EXISTS episodes (
	username TEXT NOT NULL,
	device TEXT,

	podcast TEXT NOT NULL,
	episode TEXT NOT NULL,

	timestamp INTEGER, -- timestamp
	guid TEXT,
	action TEXT NOT NULL,
	started INTEGER,
	position INTEGER,
	total INTEGER,

	-- metadata
	modified INTEGER NOT NULL,

	UNIQUE(username, podcast, episode)
);
