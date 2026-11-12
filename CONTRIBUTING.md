# Contributing to Keymotion

Thanks for helping. You can write in English or Turkish (*Türkçe yazabilirsiniz*).

The full guide, with setup steps and how the code is organised, is on the site: [Contributing](https://cebersaban44-rgb.github.io/keymotion/developers/contributing/) ([Türkçe](https://cebersaban44-rgb.github.io/keymotion/tr/developers/contributing/)).

## Ways to help

- **Report a bug** with the [bug form](https://github.com/cebersaban44-rgb/keymotion/issues/new?template=bug.yml). In the app, **Settings** has a button that copies the diagnostic information the form asks for.
- **Have another keyboard?** Plug it in. Keymotion says it isn't supported yet and offers **Request support**, which copies the keyboard's details without writing anything to it. Paste them into the [keyboard form](https://github.com/cebersaban44-rgb/keymotion/issues/new?template=keyboard.yml). The [supported keyboards](https://cebersaban44-rgb.github.io/keymotion/help/keyboards/) page lists the ones we know about.
- **Questions and ideas** go to [Discussions](https://github.com/cebersaban44-rgb/keymotion/discussions).
- **Security problems** are reported privately; see [SECURITY.md](SECURITY.md).

## Pull requests

- Open pull requests against **`develop`**. `main` holds the released code and only changes when a version is released.
- Keep a pull request to one change, and say how you tested it.
- The checks must pass: `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, the TypeScript build, ESLint, the `tools/check-*.mjs` scripts and `cargo deny`. CI runs all of them.
- If you change something people see, update the user guide in `site/` in both English and Turkish.

## The keyboard comes first

- Only send the keyboard commands that have been measured on that keyboard. Firmware, bootloader and over-the-air update paths are blocked in the code and stay blocked.
- Anything that writes to the keyboard happens because the user asked for it.
- The keyboard's factory screen animation and the vendor's driver files are never added to the repository (`tools/check-vendor-files.mjs` checks this).

## License

Keymotion is licensed under MIT or Apache-2.0, at your option. Unless you say otherwise, a contribution you submit is licensed the same way, without any additional terms.

By taking part you agree to the [code of conduct](CODE_OF_CONDUCT.md).
