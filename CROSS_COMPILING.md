# Cross Compiling

While `podsync` can be cross-compiled, there are steps to make this easier.

Using [rustls] helps avoid a native [OpenSSL] build, which then leaves just two (indirect) dependencies that need to be cross-compiled:
- `libsqlite3-sys`
- `ring`

These require a cross compiler (and in at least `ring`'s case, C development headers/libraries), for example `arm-linux-gnueabihf-gcc` or `clang -target arm-linux-gnueabihf`.

After that, run `cargo build --target armv7-unknown-linux-gnueabihf` for an armv7 binary on a Linux host (e.g. raspberry pi), or use [`cross`]:

```sh
cargo install cross
cross build --target armv7-unknown-linux-gnueabihf
```

[rustls]: https://crates.io/crates/rustls
[OpenSSL]: https://crates.io/crates/openssl-sys
[`cross`]: https://crates.io/crates/cross
