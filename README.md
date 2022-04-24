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

Copyright 2022-present Predrag Gruevski.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at
[http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

The present date is determined by the timestamp of the most recent commit in the repository.
By accessing, and contributing code, comments, or issues to this repository,
you are agreeing that all your contributions may be used, modified, copied, and/or redistributed
under any terms chosen by the original author and/or future maintainers of this project.
