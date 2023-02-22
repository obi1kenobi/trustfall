# Querying the HackerNews API with `trustfall`

This is a demo showing how to plug in a real-world API into `trustfall`
and execute queries against it.

The key idea demonstrated in this demo is **composition in schemas and queries**.

Even without using `trustfall` capabilities, a motivated programmer could write
a purpose-built tool that directly uses the HackerNews API to perform the operations
examined here. However, each such tool would need to be written, maintained, and
optimized separately. Tools that do related-but-different operations, especially
if implemented by different people or at different points in time, are unlikely
to share code. For tools performing queries of significant complexity, the large state space
means the tool is also unlikely to be thoroughly tested, and may be buggy and difficult to extend.

In contrast, querying using the `trustfall` engine allows all functionality to be decomposed
into smaller components (vertices, edges, vertex properties), each able to be implemented
and tested independently of others. Each component represents one conceptual operation,
for example the "get a `User` vertex's `name`" property operation, or the "for a `Comment` vertex,
find the `User` that authored that comment" edge operation. Each such component can be added
to the schema one at a time, and implemented and tested individually.

It's much easier to implement and test multiple small components, than to build and test
all possible compositions. If each component is correctly implemented, the `trustfall` query engine
guarantees that the composition of such components is also going to be correct.
This makes it possible to confidently expose larger schemas and execute more complex queries
than previously possible, without worrying about bugs. Similarly, the composition-based
implementation approach allows individual operations to be optimized as necessary,
making it easier to win performance gains above and beyond the very good performance
already provided by the iterator-style execution model.

## Table of Contents
- [Components](#components)
- [Installing Rust](#installing-rust)
- [Running queries](#running-queries)
   - [Query syntax primer](#query-syntax-primer)
- [Examples](#examples)
   - [Example: Front page stories with links](#example-front-page-stories-with-links)
   - [Example: Jobs in the top 50 items](#example-jobs-in-the-top-50-items)
   - [Example: Latest links submitted by high-karma users](#example-latest-links-submitted-by-high-karma-users)
   - [Example: Latest links with high-karma commenters](#example-latest-links-with-high-karma-commenters)
- [Writing your own queries](#writing-your-own-queries)

## Components

The project consists of the following components:
- `vertex.rs` defines the `Vertex` enum which `trustfall` uses to
  represent vertices in the query graph.
- `adapter.rs` defines the `HackerNewsAdapter` struct, which implements
  the `trustfall::provider::BasicAdapter` trait and connects the query engine
  to the HackerNews API.
    - The `resolve_starting_vertices` method is what produces the initial iterator of `Vertex` vertices
      corresponding to the root edge at which querying starts (e.g. `FrontPage`).
    - The `resolve_property` method is used to get property values for each `Vertex` in an iterator.
    - The `resolve_neighbors` method is used to get the neighboring vertices (`Vertex`s)
      across a particular edge, for each `Vertex` in an iterator.
    - The `resolve_coercion` method is kind of like the Python `isinstance()` function:
      for each `Vertex` in an iterable, it checks whether that `Vertex`'s type can be narrowed
      to a more derived type than it previously represented. For example, if the `Vertex` originally
      represented `interface Animal`, `resolve_coercion` may be used to check whether the `Vertex`
      is actually of `type Dog implements Animal`.
- `main.rs` is a simple CLI app that can execute query files in `ron` format.

## Installing Rust

This demo requires the Rust toolchain. To install Rust, follow the
[official instructions](https://www.rust-lang.org/tools/install). On UNIX-like operating systems,
that's usually as simple as `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.

To confirm everything is set up correctly, `cd` into this directory and run `cargo check`.
After compiling for a minute or two, it should find no errors.

## Running queries

This demo contains several example query files in the `example_queries` directory.
Each file represents a single query (conforming to the schema in `hackernews.graphql`)
together with any arguments necessary to run the query.

To execute a query, run `cargo run --example hackernews query path/to/query/file.ron`. The execution is
lazy and incremental (iterator-style), so you'll see results stream onto the screen
continuously as they are received.

**Reminder**: While the schema and query syntax here is able to be parsed with GraphQL,
you are not actually using a GraphQL API, and the query semantics are *not* the same as GraphQL.
GraphQL's capabilities are a strict subset of the abilities of the query capabilities
of `trustfall`.

### Query syntax primer

Here are the most notable aspects in which the `trustfall` query language differs from GraphQL:
- `@output` is used to mark fields for output; in SQL, this would correspond to a term
  in the `SELECT` clause.
- `@filter` denotes that a field must match a predicate; in SQL, this would correspond to
  a term in the `WHERE` clause.
- Expanding an edge by default has semantics equivalent to SQL `INNER JOIN`.
  Directives like `@optional / @recurse / @fold` may be applied to edges to change their behavior.
- When an edge points to an `interface` type, it is possible to select only a specific subtype
  (`interface` or `type`) of that `interface` using a type coercion: `... on Foo` to select
  only `Foo`-typed vertices and discard all others.
- `@optional` denotes that an edge is optional and is allowed to not exist. This is semantically
  equivalent to a SQL `LEFT JOIN`.
- `@recurse` denotes that an edge is to be traversed recursively between 0 and the number of times
  specified in the directive's `depth` field.
- `@fold` requests that the data output on the other side of the edge be "folded" into lists
  for each output field. In PostgreSQL terminology, this is like a `GROUP BY` with `array_agg()`
  applied to all folded outputs.

## Examples

Let's describe and explain the queries in the `example_queries` directory.

### Example: Front page stories with links

`cargo run --example hackernews query example_queries/front_page_stories_with_links.ron` gets the HackerNews
items on the front page that are stories with links (as opposed to job links, or submissions
like "Show HN" that contain a message instead of a link). For each match, the query outputs
its title, link, current score, the name of its submitter and their current karma.

This is what the query looks like:
```graphql
{
    FrontPage {
        ... on Story {
            title @output
            url @filter(op: "is_not_null") @output
            score @output

            byUser {
                submitter: id @output
                submitter_karma: karma @output
            }
        }
    }
}
```

Here's what running it looks like:
```
$ cargo run --example hackernews query example_queries/front_page_stories_with_links.ron
    Finished dev [unoptimized + debuginfo] target(s) in 0.15s
     Running `/.../hackernews query example_queries/front_page_stories_with_links.ron`

{
  "submitter_karma": 13731,
  "submitter": "0xedb",
  "title": "New Year, New CEO",
  "url": "https://signal.org/blog/new-year-new-ceo/",
  "score": 474
}

{
  "submitter": "danso",
  "score": 71,
  "title": "In first, US surgeons transplant pig heart into human patient",
  "submitter_karma": 142723,
  "url": "https://apnews.com/article/pig-heart-transplant-6651614cb9d73bada8eea2ecb6449aef"
}

<... many more results ...>
```

### Example: Jobs in the top 50 items

`cargo run query example_queries/jobs_in_top_50.ron` gets the jobs that are currently shown in the
top 50 items on HackerNews. For each match, it returns the posting's title, url, and current score.

This is the query:
```graphql
{
    Top(max: 50) {
        ... on Job {
            title @output
            url @filter(op: "is_not_null") @output
            score @output
        }
    }
}
```

Here's its output:
```
$ cargo run --example hackernews query example_queries/jobs_in_top_50.ron
    Finished dev [unoptimized + debuginfo] target(s) in 0.14s
     Running `/.../hackernews query example_queries/jobs_in_top_50.ron`

{
  "title": "Flow Club (YC S21) is hiring our first marketer",
  "score": 1,
  "url": "https://flowclub.notion.site/Work-at-Flow-Club-1e6cc84bfc0d4463ab333ee9bc02c46a"
}
```

### Example: Latest links submitted by high-karma users

`cargo run --example hackernews query example_queries/latest_links_by_high_karma_users.ron` gets the latest links
(i.e. links on the "new" tab) that were submitted by users with karma of 10,000 or more.
For each match, it returns the submission's title, URL, current score, and the submitter's username
and current karma.

This is the query:
```graphql
{
    LatestStory(max: 100) {
        title @output
        url @filter(op: "is_not_null") @output
        score @output

        byUser {
            submitter: id @output
            submitter_karma: karma @filter(op: ">=", value: ["$min_karma"]) @output
        }
    }
}
```
It is executed with the following arguments, shown here in RON serialization format:
```
{
    "min_karma": Uint64(10000),
}
```

Here's what running it looks like:
```
$ cargo run --example hackernews query example_queries/latest_links_by_high_karma_users.ron
    Finished dev [unoptimized + debuginfo] target(s) in 0.15s
     Running `/.../hackernews query example_queries/latest_links_by_high_karma_users.ron`

{
  "submitter_karma": 23927,
  "score": 2,
  "url": "https://github.com/snapview/sunrise",
  "title": "Sunrise: Spreadsheet-like dataflow programming in TypeScript",
  "submitter": "wslh"
}

{
  "title": "The case for Rust as the future of JavaScript infrastructure",
  "submitter_karma": 30051,
  "score": 1,
  "submitter": "feross",
  "url": "https://thenewstack.io/the-case-for-rust-as-the-future-of-javascript-infrastructure/"
}

<... many more results ...>
```

### Example: Latest links with high-karma commenters

`cargo run --example hackernews query example_queries/links_with_high_karma_commenters.ron` looks at the latest
100 story submissions (i.e. HN's "new" tab items), and selects those that have links
and also have comments (looking up to 5 reply levels deep) made by users with at least 10,000 karma.
For each match, it outputs the submission's title, current score, URL, as well as
the matching comment's content, author, and the author's current karma.

This is the query:
```graphql
{
    LatestStory(max: 100) {
        title @output
        url @filter(op: "is_not_null") @output
        score @output

        comment {
            reply @recurse(depth: 5) {
                comment: text @output

                byUser {
                    commenter: id @output
                    commenter_karma: karma @filter(op: ">=", value: ["$min_karma"]) @output
                }
            }
        }
    }
}
```
It is executed with the following arguments, shown here in RON serialization format:
```
{
    "min_karma": Uint64(10000),
}
```

Here's what running it looks like:
```
$ cargo run --example hackernews example_queries/links_with_high_karma_commenters.ron
    Finished dev [unoptimized + debuginfo] target(s) in 0.17s
     Running `/.../hackernews query example_queries/links_with_high_karma_commenters.ron`

{
  "commenter_karma": 22774,
  "url": "https://www.phoronix.com/scan.php?page=news_item&px=Intel-New-CCG-Leader",
  "comment": "&gt;Holthaus replaces EVP Gregory Bryant (“GB”), who will leave the company at the end of January for a new opportunity.<p>This is strange because Gregory Bryant was still presenting at CES [1] .<p>[1] <a href=\"https:&#x2F;&#x2F;www.anandtech.com&#x2F;show&#x2F;17171&#x2F;intel-keynote-and-svp-greg-bryant-at-ces-2022-live-blog-10am-pt-1800-utc\" rel=\"nofollow\">https:&#x2F;&#x2F;www.anandtech.com&#x2F;show&#x2F;17171&#x2F;intel-keynote-and-svp-g...</a>",
  "title": "Intel Announces New Leader of Client Computing Group",
  "commenter": "ksec",
  "score": 1
}

{
  "commenter": "scrollaway",
  "commenter_karma": 25466,
  "comment": "4 petabytes eh. Bonus points for the first article in several years to use CD-ROMs as a unit of comparison.<p>&gt; <i>&quot;drawn from crime reports, hacked from encrypted phone services and sampled from asylum seekers never involved in any crime&quot;</i>",
  "url": "https://www.theguardian.com/world/2022/jan/10/a-data-black-hole-europol-ordered-to-delete-vast-store-of-personal-data",
  "title": "A data ‘black hole’: Europol ordered to delete vast store of personal data",
  "score": 31
}

<... many more results ...>
```

## Writing your own queries

The easiest way to write and run your own query is to:
- copy the content of one of the example queries,
- edit the query string and/or arguments as necessary,
- save it to a new file,
- then run it with `cargo run --example hackernews query <your_query_file>`.

The query must use properties, types, and edges from the schema in the `hackernews.graphql` file.
