CREATE TABLE IF NOT EXISTS episodes (
	username TEXT NOT NULL,
	device TEXT NOT NULL,

	podcast TEXT NOT NULL,
	episode TEXT NOT NULL,
	timestamp DATETIME NOT NULL,
	guid TEXT,
	action TEXT NOT NULL,
	started INTEGER,
	position INTEGER,
	total INTEGER,

	UNIQUE(username, device, podcast, episode)
);
