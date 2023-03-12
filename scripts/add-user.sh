#!/bin/sh

usage(){
	echo >&2 "Usage: $0 username"
	exit 2
}

sha_hash(){
	set +e
	if command -v sha256sum >/dev/null 2>&1
	then sha256sum
	elif command -v shasum >/dev/null 2>&1
	then shasum -a 256
	else
		echo >&2 "$0: no shasum found"
		exit 1
	fi | cut -d' ' -f1
	set -e
}

if test $# -ne 1
then usage
fi

set -e

user=$1
printf "%s's password: " "$user"
stty -echo; read pass; echo; stty echo

hash=$(printf '%s' "$pass" | sha_hash)

sqlite3 pod.sql <<!
INSERT INTO users
VALUES ("$user", "$hash", NULL);
!
