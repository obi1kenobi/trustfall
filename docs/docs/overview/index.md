# Overview of the Trustfall query engine

Trustfall's goal is to turn every piece of data into a database that can be explored, queried, and combined with other databases.

Querying shouldn't be reserved just for SQL and NoSQL databases! Trustfall offers a query language that can be used over any kind of data source, such as REST APIs, JSON files, source code repositories.

For example: the [`cargo-semver-checks` linter](https://github.com/obi1kenobi/cargo-semver-checks) uses Trustfall queries to catch and prevent breaking changes that violate [semantic versioning](https://semver.org/) (SemVer) in the Rust programming language. Its queries operate over a data model derived from JSON and TOML data. An example of such a query is "find functions that have been removed from the package's public API."

## Comparing Trustfall to GraphQL

Trustfall aims to be more expressive and more performant than GraphQL. It supports functionality like recursive and left joins, aggregations, arbitrary filter clauses, and lazy evaluation —— none of which are in GraphQL. It also performs well with high query complexities: real-world Trustfall use cases can often have deeply nested or widely-branching queries covering 20 edges (joins) or more.

GraphQL is superior when aiming to drive a UI, especially one written in React. GraphQL's nested, eagerly-evaluated output is preferable here: UI queries are usually small and simple, without too much branching nor many levels of nesting.

## Comparing Trustfall to SQL

Trustfall aims to be better suited for querying data that *isn't* already available in SQL database format, such as data in REST APIs, JSON files, source code repositories, or AI tools. Its lazy evaluation model and flexible optimization API excel when getting query data is expensive in performance or monetary cost, such as when using pay-per-request or rate-limited APIs. These same properties also make Trustfall excellent at joining multiple data sources together.

If all required data is already in a SQL database, SQL will offer superior performance and should be preferred over Trustfall.
