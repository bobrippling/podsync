CREATE TABLE IF NOT EXISTS subscriptions (
	username TEXT NOT NULL,
	device TEXT NOT NULL,

	url TEXT NOT NULL,

	-- metadata
	created INTEGER NOT NULL, -- timestamp
	deleted INTEGER, -- timestamp, if null, subscription is active

	UNIQUE(url, username, device, deleted)
);
