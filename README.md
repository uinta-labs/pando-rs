
Testing Commands

```shell
balena kill browser || true
balena rm browser || true
balena run -d --privileged --rm --name browser -e 'KIOSK=1' -e 'LAUNCH_URL=https://cataas.com/cat' -e 'AUTO_REFRESH=90' -e 'ENABLE_GPU=1' -p 5011:5011 -p 35173:35173 bh.cr/balenalabs/browser-aarch64
```


### Configuration

Currently using Flox to manage the environment. The `.flox/env/manifest.toml` contains a config that was taken from here: https://flox.dev/docs/cookbook/languages/rust/#what-do-i-need-for-a-basic-environment

Configuring flox with only `cargo` is woefully insufficient. There is also no `rust` package.

To get RustRover to accept my configuration, I had to update the `Toolchain Location` to point to `/nix/store/f2c7fkd6wbs3hxql2jz6rap0vbiqwxp6-environment-develop/bin`.

This can be found by running the following:
```shell
# previously: flox activate
$ which rustc
/nix/store/f2c7fkd6wbs3hxql2jz6rap0vbiqwxp6-environment-develop/bin/rustc
```

Finally, to get the IDE fully happy I also had to define the `Standard Library Path`. This one seemed trickier to determine, but is actually really simple.

```shell
# previously: flox activate
$ echo $RUST_SRC_PATH
/Users/isaac/Dev/pando-rs/.flox/run/aarch64-darwin.pando-rs.dev
```

Finally, to get Tonic working, I had to define my path to the Protoc compiler. I did this by defining
```shell
PROTOC=/Users/isaac/Dev/pando-rs/.flox/run/aarch64-darwin.pando-rs.dev/bin/protoc
```

For the `Rust` `Run` target.
