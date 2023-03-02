#!/bin/sh

sql(){
	sqlite3 pod.sql
}

sqlx database create --database-url sqlite://pod.sql

ls migrations |
	sort -n |
	while read f; do
		!sqlite3 pod.sql <"$f"
	done
