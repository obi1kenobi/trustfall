# `trustfall` query engine and examples

Directory registry:
- `trustfall_core` contains the query engine itself.
- `pytrustfall` contains `pyo3`-based Python bindings for the `trustfall` engine.
- `demo-hackernews` contains an example use case: querying the HackerNews APIs.
- `demo-feeds` is an example implementation querying RSS feeds using Rust and `trustfall`.
- `demo-metar` is an example implementation querying METAR aviation weather reports using Rust
  and `trustfall`.
- `filetests_proc_macro` is a procedural macro used to generate test cases defined by files:
  they ensure that the function under test, when given an input specified by one file,
  produces an output equivalent to the contents of another file.

For a "getting started" overview, look at `demo-hackernews/README.md`.

Copyright 2022-present Predrag Gruevski. Confidential and proprietary. All rights reserved.
The present date is determined by the timestamp of the most recent commit in the repository.
By accessing, and contributing code, comments, or issues to this repository,
you are agreeing that all your contributions may be used, modified, copied, and/or redistributed
under any terms chosen by the original author and/or future maintainers of this project.
