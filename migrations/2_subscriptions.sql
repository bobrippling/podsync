CREATE TABLE IF NOT EXISTS subscriptions (
	url TEXT NOT NULL,
	username TEXT NOT NULL,
	device TEXT NOT NULL, -- TODO: id / fk or name?
	UNIQUE(url, username, device)
);
