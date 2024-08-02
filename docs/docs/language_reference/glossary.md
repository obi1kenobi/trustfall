# Glossary

Let's define the meaning of common pieces of Trustfall terminology.

This page references a hypothetical social media app ("Flutter") as an example with which to explain Trustfall concepts. In Flutter, users may post public messages, "like" each other's posts, and follow other users' activity.

### schema

A formal description of the shape of a data set.

The schema is used to validate and type-check queries, making sure they are valid before attempting to execute them. It also powers IDE conveniences such as code auto-complete.

It describes different [types of vertices](#vertex-type), each of which contains [properties](#property) and may be connected to other [vertices](#vertex) via named [edges](#edge).

### vertex

A single data item in a Trustfall dataset. In our Flutter example app, each user's profile would be represented as a separate vertex.

Its shape is described by its [vertex type](#vertex-type): it contains a set of [properties](#property), and may be connected to other vertices via [edges](#edge).

The equivalent concept in SQL is a row in a table.

### vertex type

A shape of [vertex](#vertex) data items, as described in the [schema](#schema).

In our Flutter example app, `User` and `Post` are two of the vertex types. The `User` type is defined like this:

```graphql
type User {
    # properties
    username: String!]]]
    display_name: String
    # ...

    # edges
    posts: [Post!]
    follows: [User!]
    # ...
}
```

The equivalent concept in SQL is a table.

### property

A named and typed value that each [vertex](#vertex) of a given [vertex type](#vertex-type) contains.

In our Flutter example app, the `User` vertex type has properties like `username` of type `String!` (non-nullable string).

The equivalent concept in SQL is a column.

### edge

A named relationship that one [vertex type](#vertex-type) might have to another [vertex type](#vertex-type), or an instance of such a relationship between two specific [vertices](#vertex).

In our Flutter example app, the `User` [vertex type](#vertex-type) has a `posts` edge that represents the messages that a user has posted. Each `User` [vertex](#vertex) may be connected to zero, one, or more `Post` [vertices](#vertex) via instances of the `posts` edge.

The equivalent concept in SQL is a foreign key relationship.

### entrypoint

A starting point for Trustfall queries in a given [schema](#schema).

In our Flutter example app, entrypoints include:
- Looking up a `User` [vertex](#vertex) by their username.
- Listing messages that are currently popular.

SQL doesn't have an equivalent concept since querying may begin at any table. Trustfall supports querying any data source, not just SQL, and many data sources do not allow listing all instances of a particular data point. For example, GitHub doesn't support enumerating all its millions of registered users, so the equivalent of a SQL `SELECT * FROM GitHubUser` query is not possible against the GitHub APIs. In Trustfall, we model these querying restrictions by separating the [vertex types](#vertex-type) from the entrypoints where querying may begin.

### adapter

A Trustfall plugin that enables querying a specific dataset with the Trustfall query interpreter. It acts as a connector between the Trustfall APIs and the underlying data source, which might be a file format, an API, a database, etc.

### directive

A component of a query that describes an operation to be performed, such as filtering or transforming the data. Directives are prefixed with the `@` symbol: `@filter`, `@output`, etc.
