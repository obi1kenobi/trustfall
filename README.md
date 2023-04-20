# Trustfall — Engine for Querying (Almost) Everything

Trustfall is a query engine for querying any kind of data source, from APIs and databases
to any kind of files on disk — and even AI models.

* [Try Trustfall in your browser](#try-trustfall-in-your-browser)
* [10min tech talk + demo](#10min-tech-talk--demo)
* [Examples of querying real-world data with Trustfall](#examples-of-querying-real-world-data-with-trustfall)

## Try Trustfall in your browser

The Trustfall Playground supports running queries against public data sources such as:
- the HackerNews REST APIs: https://play.predr.ag/hackernews
- the rustdoc JSON of top Rust crates: https://play.predr.ag/rustdoc

For example,
[this link](https://play.predr.ag/hackernews#?f=1&q=IyBDcm9zcyBBUEkgcXVlcnkgKEFsZ29saWEgKyBGaXJlYmFzZSk6CiMgRmluZCBjb21tZW50cyBvbiBzdG9yaWVzIGFib3V0ICJvcGVuYWkuY29tIiB3aGVyZQojIHRoZSBjb21tZW50ZXIncyBiaW8gaGFzIGF0IGxlYXN0IG9uZSBHaXRIdWIgb3IgVHdpdHRlciBsaW5rCnF1ZXJ5IHsKICAjIFRoaXMgaGl0cyB0aGUgQWxnb2xpYSBzZWFyY2ggQVBJIGZvciBIYWNrZXJOZXdzLgogICMgVGhlIHN0b3JpZXMvY29tbWVudHMvdXNlcnMgZGF0YSBpcyBmcm9tIHRoZSBGaXJlYmFzZSBITiBBUEkuCiAgIyBUaGUgdHJhbnNpdGlvbiBpcyBzZWFtbGVzcyAtLSBpdCBpc24ndCB2aXNpYmxlIGZyb20gdGhlIHF1ZXJ5LgogIFNlYXJjaEJ5RGF0ZShxdWVyeTogIm9wZW5haS5jb20iKSB7CiAgICAuLi4gb24gU3RvcnkgewogICAgICAjIEFsbCBkYXRhIGZyb20gaGVyZSBvbndhcmQgaXMgZnJvbSB0aGUgRmlyZWJhc2UgQVBJLgogICAgICBzdG9yeVRpdGxlOiB0aXRsZSBAb3V0cHV0CiAgICAgIHN0b3J5TGluazogdXJsIEBvdXRwdXQKICAgICAgc3Rvcnk6IHN1Ym1pdHRlZFVybCBAb3V0cHV0CiAgICAgICAgICAgICAgICAgICAgICAgICAgQGZpbHRlcihvcDogInJlZ2V4IiwgdmFsdWU6IFsiJHNpdGVQYXR0ZXJuIl0pCgogICAgICBjb21tZW50IHsKICAgICAgICByZXBseSBAcmVjdXJzZShkZXB0aDogNSkgewogICAgICAgICAgY29tbWVudDogdGV4dFBsYWluIEBvdXRwdXQKCiAgICAgICAgICBieVVzZXIgewogICAgICAgICAgICBjb21tZW50ZXI6IGlkIEBvdXRwdXQKICAgICAgICAgICAgY29tbWVudGVyQmlvOiBhYm91dFBsYWluIEBvdXRwdXQKCiAgICAgICAgICAgICMgVGhlIHByb2ZpbGUgbXVzdCBoYXZlIGF0IGxlYXN0IG9uZQogICAgICAgICAgICAjIGxpbmsgdGhhdCBwb2ludHMgdG8gZWl0aGVyIEdpdEh1YiBvciBUd2l0dGVyLgogICAgICAgICAgICBsaW5rCiAgICAgICAgICAgICAgQGZvbGQKICAgICAgICAgICAgICBAdHJhbnNmb3JtKG9wOiAiY291bnQiKQogICAgICAgICAgICAgIEBmaWx0ZXIob3A6ICI%2BPSIsIHZhbHVlOiBbIiRtaW5Qcm9maWxlcyJdKQogICAgICAgICAgICB7CiAgICAgICAgICAgICAgY29tbWVudGVySURzOiB1cmwgQGZpbHRlcihvcDogInJlZ2V4IiwgdmFsdWU6IFsiJHNvY2lhbFBhdHRlcm4iXSkKICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICBAb3V0cHV0CiAgICAgICAgICAgIH0KICAgICAgICAgIH0KICAgICAgICB9CiAgICAgIH0KICAgIH0KICB9Cn0%3D&v=ewogICJzaXRlUGF0dGVybiI6ICJodHRwW3NdOi8vKFteLl0qXFwuKSpvcGVuYWkuY29tLy4qIiwKICAibWluUHJvZmlsZXMiOiAxLAogICJzb2NpYWxQYXR0ZXJuIjogIihnaXRodWJ8dHdpdHRlcilcXC5jb20vIgp9)
shows the results of the HackerNews query: "Which GitHub or Twitter
users are commenting on stories about OpenAI?"

In the Playground, Trustfall is configured to run client-side as WASM, performing
all aspects of query processing (parsing, compilation, and execution) within the browser.
While this demo highlights Trustfall's ability to be embedded within a target application,
it is of course able to be used in a more traditional client-server context as well.

## 10min tech talk + demo

Trustfall was featured in the ["How to Query (Almost) Everything" talk](https://www.hytradboi.com/2022/how-to-query-almost-everything)
talk at the [HYTRADBOI 2022](https://www.hytradboi.com/) conference.

![Terminal recording of running `cargo run --release -- query example_queries/actions_in_repos_with_min_10_hn_pts.ron` in the `demo-hytradboi` demo project. The system returns the first 20 results of the query in 6.36 seconds."](./demo-hytradboi/query-demo.gif)

*Demo from the talk showing the execution of the cross-API query: "Which GitHub Actions are used in projects on the front page of HackerNews with >=10 points?"*

The demo executes the following query across the HackerNews and GitHub APIs and over the YAML-formatted GitHub repository workflow files:
```graphql
{
  HackerNewsTop(max: 200) {
    ... on HackerNewsStory {
      hn_score: score @filter(op: ">=", value: ["$min_score"]) @output

      link {
        ... on GitHubRepository {
          repo_url: url @output

          workflows {
            workflow: name @output
            workflow_path: path @output

            jobs {
              job: name @output

              step {
                ... on GitHubActionsImportedStep {
                  step: name @output
                  action: uses @output
                }
              }
            }
          }
        }
      }
    }
  }
}
```

Instructions for
running the demo are available together with the source code in the
`demo-hytradboi` directory: [link](./demo-hytradboi).

## Examples of querying real-world data with Trustfall

- [HackerNews APIs](./trustfall/examples/hackernews/), including an overview of the query language
  and an example of querying REST APIs.
- [RSS/Atom feeds](./trustfall/examples/feeds/), showing how to query structured data
  like RSS/Atom feeds.
- [airport weather data (METAR)](./trustfall/examples/weather), showing how to query CSV data from
  aviation weather reports.

Trustfall also powers the [`cargo-semver-checks`](https://crates.io/crates/cargo-semver-checks)
semantic versioning linter.
More details on the role Trustfall plays in that use case are available in
[this blog post](https://predr.ag/blog/speeding-up-rust-semver-checking-by-over-2000x/).

## Using Trustfall over a new data source

The easiest way to plug in a new data source is by implementing
[the `BasicAdapter` trait](https://docs.rs/trustfall_core/latest/trustfall_core/interpreter/basic_adapter/trait.BasicAdapter.html).

Python bindings are available, and are built automatically on every change to
the engine; the most recent version may be downloaded
[here](https://github.com/obi1kenobi/trustfall/releases). A getting started
guide for Python is forthcoming ([tracking
issue](https://github.com/obi1kenobi/trustfall/issues/16)); in the meantime, the
best resource is the Python bindings' [test suite](./pytrustfall/trustfall/tests/test_execution.py).

## Directory Registry

- [`trustfall`](./trustfall/) is a façade crate. This is the preferred way to use Trustfall.
- [`trustfall_core`](./trustfall_core/) contains the query engine internals
- [`trustfall_derive`](./trustfall_derive/) defines macros that simplify plugging in data sources.
- [`pytrustfall`](./pytrustfall/) contains Trustfall's Python bindings
- [`trustfall_wasm`](./trustfall_wasm/) is a WASM build of Trustfall
- [`trustfall_filetests_macros`](./trustfall_filetests_macros/) is a procedural
  macro used to generate test cases defined by files: they ensure that the
  function under test, when given an input specified by one file, produces an
  output equivalent to the contents of another file.
- [`experiments`](./experiments/) contains various experimental projects
  such as the [Trustfall web playground](https://play.predr.ag/).

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
