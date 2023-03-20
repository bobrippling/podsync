ROOT = ../..
TARGET = release
BIN_ROOT = ${ROOT}/target/${TARGET}

install: \
	/usr/local/bin/podsync \
	/etc/init.d/podsync \
	/etc/logrotate.d/podsync

stripped: ${BIN_ROOT}/podsync.stripped

/usr/local/bin/podsync: ${BIN_ROOT}/podsync.stripped
	install -o0 -g0 -m755 "$<" "$@"

${BIN_ROOT}/podsync.stripped: ${BIN_ROOT}/podsync
	strip -s "$<" -o "$@"

/etc/init.d/podsync: ${ROOT}/contrib/debian/podsync
	install -o0 -g0 -m755 "$<" "$@"

/etc/logrotate.d/podsync: logrotate.conf
	install -o0 -g0 -m644 "$<" "$@"

uninstall:
	rm -f \
		/usr/local/bin/podsync \
		/etc/init.d/podsync \
		/etc/logrotate.d/podsync

.PHONY: install stripped
