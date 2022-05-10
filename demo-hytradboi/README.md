# HYTRADBOI 2022 `trustfall` demo

The code in this directory is the demo for the "How to query (almost) everything" talk
from [HYTRADBOI 2022](https://www.hytradboi.com/).

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

## Running the demo

This code requires a Rust 1.59+ toolchain, which on UNIX-based operating systems can be installed
with `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`. For other operating systems,
follow the [official Rust instructions](https://www.rust-lang.org/tools/install).

If you already have a Rust toolchain, but it's a version older than 1.59, it's recommended to
upgrade it by running `rustup upgrade`.

Querying the GitHub API requires a personal access token, which is easy to get using your
GitHub account:
https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token

Once you've installed Rust and obtained a personal access token,
execute the following code to download and compile the demo code:
```bash
git clone git@github.com:obi1kenobi/trustfall.git
cd trustfall/demo-hytradboi
cargo build --release
export GITHUB_TOKEN="< ... your GitHub token goes here ... >"
```

You are now ready to run the demo, which shows the imported GitHub Actions used in repos
that have 10+ points on the HackerNews top stories page:
```bash
cargo run --release -- query example_queries/actions_in_repos_with_min_10_hn_pts.ron
```

[Six example queries are included](https://github.com/obi1kenobi/trustfall/tree/main/demo-hytradboi/example_queries)
in the `example_queries` directory, querying various combinations of data across HackerNews,
crates.io, and GitHub. For example:
- Here are the imported GitHub Actions used by the most-downloaded Rust crates on crates.io:
```bash
cargo run --release -- query example_queries/crates_io_github_actions.ron
```
- Here is [Patrick McKenzie ("patio11")](https://twitter.com/patio11) commenting on HackerNews
stories about his own blog posts:
```bash
cargo run --release -- query example_queries/hackernews_patio11_own_post_comments.ron
```

Of course, these examples only scratch the surface of what is possible!
You can write and execute any queries you'd like so long as they validate against the schema defined
in [this file](https://github.com/obi1kenobi/trustfall/blob/main/demo-hytradboi/schema.graphql).

Happy querying!
