# Contributing

Thank you for helping. PenaDF aims to stay a small offline tool for everyday office PDF work.

## Reporting a problem

Open an issue with:

* what you did, what you expected, what happened;
* your Windows version and the PenaDF version (Help → About);
* if possible, a PDF that shows the problem. Remove anything private first.

## Changing code

1. Build and test on Linux/WSL: `scripts/test.sh`. On Windows, set
   `PENADF_PDFIUM_DIR=vendor\pdfium\windows` and run `cargo test`.
2. Keep document logic in `src/ops/` as plain functions over PDF bytes, with a test in
   `tests/ops.rs`. UI code goes in `src/gui/` (testable) or `src/ui/` (the window).
3. Before opening a pull request, run `cargo fmt` and
   `cargo clippy --all-targets -- -D warnings`.
4. New dependencies must use a licence listed in `about.toml`.

## Scope

Welcome: fixes, speed, accessibility, translations, and features office users need (forms,
signing, organizing, OCR).

Out of scope for the core app: anything that sends data online without the user asking, ads,
and tracking.

## Licence of contributions

By contributing you agree your contribution is licensed under the Apache License 2.0, the same as
the project (section 5 of the licence).
