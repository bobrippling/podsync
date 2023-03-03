-- TODO: unique?

CREATE TABLE IF NOT EXISTS devices (
	caption TEXT NOT NULL,
	username TEXT NOT NULL,
	type TEXT NOT NULL,
	subscriptions INTEGER NOT NULL
);
