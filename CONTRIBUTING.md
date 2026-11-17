# Contributing

Thanks for your interest! Bug reports, new examples and performance work are all welcome.

## Before opening a pull request

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

If you change how something looks, run the affected example and attach a
screenshot or GIF (`--shots <dir> <times>` or `scripts/gif.sh`).

If you change rendering or simulation code, include `--bench` numbers from
before and after, along with your GPU.

## New examples

A good example shows off one library feature, stays short (roughly 100–250
lines), and accepts the shared flags with `Config::new(...).with_args()`.
Add it to the README table, and to `scripts/gifs.sh` if it should have a GIF.

## License

By contributing, you agree that your contributions will be dual licensed under
MIT OR Apache-2.0, as described in the README.
