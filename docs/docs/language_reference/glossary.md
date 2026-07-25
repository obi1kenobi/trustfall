# Glossary

Let's define the meaning of common pieces of Trustfall terminology.

This page uses the HackerNews playground schema as an example. In that schema, users submit stories, stories have comments, and comments may have replies.

### schema

A formal description of the shape of a data set.

The schema is used to validate and type-check queries, making sure they are valid before attempting to execute them. It also powers IDE conveniences such as code auto-complete.

It describes different [types of vertices](#vertex-type), each of which contains [properties](#property) and may be connected to other [vertices](#vertex) via named [edges](#edge).

### vertex

A single data item in a Trustfall dataset. In the HackerNews playground, a specific story, comment, or user profile would each be represented as a separate vertex.

Its shape is described by its [vertex type](#vertex-type): it contains a set of [properties](#property), and may be connected to other vertices via [edges](#edge).

The equivalent concept in SQL is a row in a table.

### vertex type

A shape of [vertex](#vertex) data items, as described in the [schema](#schema).

In the HackerNews playground, `User` and `Story` are two of the vertex types. A simplified `User` type is defined like this:

```graphql
type User {
    # properties
    url: String!
    id: String!
    karma: Int!
    # ...

    # edges
    submitted: [Item!]
    link: [Webpage!]
    # ...
}
```

The equivalent concept in SQL is a table.

### property

A named and typed value that each [vertex](#vertex) of a given [vertex type](#vertex-type) contains.

In the HackerNews playground, the `User` vertex type has properties like `id` of type `String!` and `karma` of type `Int!`.

The equivalent concept in SQL is a column.

### edge

A named relationship that one [vertex type](#vertex-type) might have to another [vertex type](#vertex-type), or an instance of such a relationship between two specific [vertices](#vertex).

In the HackerNews playground, the `Story` [vertex type](#vertex-type) has a `byUser` edge that represents the user who submitted the story. Each `Story` [vertex](#vertex) is connected to exactly one `User` [vertex](#vertex) via an instance of the `byUser` edge.

The equivalent concept in SQL is a foreign key relationship.

### entrypoint

A starting point for Trustfall queries in a given [schema](#schema).

In the HackerNews playground, entrypoints include:
- Looking up a `User` [vertex](#vertex) by their username.
- Listing the top HackerNews items.
- Searching for stories or comments containing specific text.

SQL doesn't have an equivalent concept since querying may begin at any table. Trustfall supports querying any data source, not just SQL, and many data sources do not allow listing all instances of a particular data point. For example, GitHub doesn't support enumerating all its millions of registered users, so the equivalent of a SQL `SELECT * FROM GitHubUser` query is not possible against the GitHub APIs. In Trustfall, we model these querying restrictions by separating the [vertex types](#vertex-type) from the entrypoints where querying may begin.

### adapter

A Trustfall plugin that enables querying a specific dataset with the Trustfall query interpreter. It acts as a connector between the Trustfall APIs and the underlying data source, which might be a file format, an API, a database, etc.

### directive

A component of a query that describes an operation to be performed, such as filtering or transforming the data. Directives are prefixed with the `@` symbol: `@filter`, `@output`, etc.
