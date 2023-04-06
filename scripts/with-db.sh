#!/bin/sh

usage(){
	echo >&2 "Usage: $0"
	echo >&2 "(this script is intended to be source'd)"
	exit 2
}

if test $# -ne 0
then usage
fi

export DATABASE_URL=sqlite://pod.sql
