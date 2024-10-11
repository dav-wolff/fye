PRAGMA foreign_keys = ON;

CREATE TABLE node_id (
	current_id BigInt PRIMARY KEY NOT NULL
);

INSERT INTO node_id (current_id) VALUES (1);

CREATE TABLE files (
	id BigInt PRIMARY KEY NOT NULL,
	size BigInt NOT NULL
);

CREATE TABLE directories (
	id BigInt PRIMARY KEY NOT NULL,
	parent BigInt NOT NULL,
	FOREIGN KEY(parent) REFERENCES directories
);

CREATE TABLE directory_entries (
	parent BigInt NOT NULL,
	name Text NOT NULL,
	directory BigInt UNIQUE,
	file BigInt UNIQUE,
	PRIMARY KEY (parent, name),
	FOREIGN KEY(parent) REFERENCES directories,
	FOREIGN KEY(directory) REFERENCES directories ON UPDATE CASCADE ON DELETE CASCADE,
	FOREIGN KEY(file) REFERENCES files ON UPDATE CASCADE ON DELETE CASCADE,
	CHECK ((directory IS NOT NULL AND file IS NULL) OR (directory IS NULL AND file IS NOT NULL))
);

INSERT INTO directories (id, parent) VALUES (1, 1);
