ROOT = ../..
TARGET = release

# strip -s <target> -o <out>
# can be used for debug builds without debug info

install: \
	/usr/local/bin/podsync \
	/etc/init.d/podsync \
	/etc/logrotate.d/podsync

/usr/local/bin/podsync: ${ROOT}/target/${TARGET}/podsync
	install -o0 -g0 -m755 "$<" "$@"

/etc/init.d/podsync: ${ROOT}/contrib/debian/podsync
	install -o0 -g0 -m755 "$<" "$@"

/etc/logrotate.d/podsync: logrotate.conf
	install -o0 -g0 -m644 "$<" "$@"

uninstall:
	rm -f \
		/usr/local/bin/podsync \
		/etc/init.d/podsync \
		/etc/logrotate.d/podsync

.PHONY: install
