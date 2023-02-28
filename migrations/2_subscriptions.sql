CREATE TABLE subscriptions (
	id INTEGER NOT NULL PRIMARY KEY,
	url TEXT NOT NULL,
	username TEXT NOT NULL,
	device TEXT NOT NULL -- TODO: id / fk or name?
);
