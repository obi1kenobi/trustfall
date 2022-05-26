# `trustfall` — How to Query (Almost) Everything

This repository contains the `trustfall` query engine, which can be used to query any data source
or combination of data sources: databases, APIs, raw files (JSON, CSV, etc.), git version control,
etc. For a 10min video introduction to the project, see
the ["How to Query (Almost) Everything" talk](https://www.hytradboi.com/2022/how-to-query-almost-everything)
from the [HYTRADBOI 2022](https://www.hytradboi.com/) conference.

![Terminal recording of running `cargo run --release -- query example_queries/actions_in_repos_with_min_10_hn_pts.ron` in the `demo-hytradboi` demo project. The system returns the first 20 results of the query in 6.36 seconds."](https://github.com/obi1kenobi/trustfall/raw/main/demo-hytradboi/query-demo.gif)

*Demo showing the execution of the cross-API query: "Which GitHub Actions are used in projects on the front page of HackerNews with >=10 points?"*

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

This demo is part of the ["How to Query (Almost) Everything" talk](https://www.hytradboi.com/2022/how-to-query-almost-everything)
from the [HYTRADBOI 2022](https://www.hytradboi.com/) conference.
Instructions for running the demo are available together with the source code
in the `demo-hytradboi` directory:
[link](https://github.com/obi1kenobi/trustfall/tree/main/demo-hytradboi).

For a brief overview of the query language and an example of using it to query APIs, check the
`demo-hackernews` directory for a simpler demo only querying the HackerNews APIs: [link](https://github.com/obi1kenobi/trustfall/tree/main/demo-hackernews).

Python bindings are available, and are built automatically on every change to the engine; the most recent version may be downloaded [here](https://github.com/obi1kenobi/trustfall/releases). A getting started guide for Python is
forthcoming ([tracking issue](https://github.com/obi1kenobi/trustfall/issues/16)); in the meantime,
the best resource is the Python bindings'
[test suite](https://github.com/obi1kenobi/trustfall/blob/main/pytrustfall/pytrustfall/tests/test_execution.py).

## Directory Registry

- `trustfall_core` contains the query engine itself: [link](https://github.com/obi1kenobi/trustfall/tree/main/trustfall_core)
- `pytrustfall` contains `pyo3`-based Python bindings for the `trustfall` engine: [link](https://github.com/obi1kenobi/trustfall/tree/main/pytrustfall)
- `demo-hackernews` contains an example use case: querying the HackerNews APIs.
- `demo-hytradboi` contains the demo code from the "How to Query (Almost) Everything" talk
  at the [HYTRADBOI 2022](https://www.hytradboi.com/) conference.
- `demo-feeds` is an example implementation querying RSS feeds using Rust and `trustfall`.
- `demo-metar` is an example implementation querying METAR aviation weather reports using Rust
  and `trustfall`.
- `filetests_proc_macro` is a procedural macro used to generate test cases defined by files:
  they ensure that the function under test, when given an input specified by one file,
  produces an output equivalent to the contents of another file.

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
