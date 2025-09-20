# Trustfall schemas

Datasets in Trustfall are described using a [schema](../glossary.md#schema). The schema describes the shape of each data point in the dataset, and how different data points are connected to each other.

The schema is used to validate and type-check queries, making sure they are valid before attempting to execute them. It also powers editor conveniences such as code auto-complete.

## Schema components overview

In Trustfall, all datasets are modelled as a graph, consisting of [typed][vertex-type] [vertices][vertex] containing [properties][property], and connected to each other by [edges][edge].

Since editor auto-complete also relies on schema data, Trustfall [directives][directive] are included in each Trustfall schema. Their definitions specify the query positions where each directive is valid and may be suggested as a code auto-completion. The schema directives section is always the same for any given Trustfall version, and must not be changed by hand.

<!-- the below is more of a guide than a reference -->

### Example setup

Consider a hypothetical social media app called "Flutter" where users may post public messages, "like" each other's posts, and follow other users' activity.

In this section, we'll build up a Trustfall schema for the Flutter app step by step.

<details>
<summary>
If you'd like to peek ahead, click here to reveal the final schema we'll build up to.
</summary>

```graphql
schema {
    query: Entrypoints
}

type Entrypoints {
    """
    Find a specific user by their username, if they exist.
    """
    FindUser(username: String!): User

    """
    Run a search across all public posts with the given search terms.
    """
    SearchPosts(text: String!): [Post!]
}

"""
A piece of data that has its own webpage.
"""
interface Webpage {
    # properties
    """
    The human-readable URL at which this webpage may be visited.
    """
    url: String!
}

"""
A user account on Flutter.
"""
type User implements Webpage {
    # properties from Webpage
    """
    The URL of this user's profile page.
    """
    url: String!

    # own properties
    """
    The user's unique username. All users are required to have one.
    """
    username: String!

    """
    The name the user would like to be known by, if any was set.

    If the user has not set a display name, this value will be `null`.
    """
    display_name: String
    # end of properties

    # edges
    """
    The messages posted by this user, if any.
    """
    posts: [Post!]

    """
    The users this user follows, if any.
    """
    follows: [User!]
}

"""
A message posted by a Flutter user.
"""
type Post implements Webpage {
    # properties from Webpage
    """
    The URL of this post.
    """
    url: String!

    # own properties
    """
    The contents of the posted message.
    """
    message: String!

    # edges
    """
    The user who authored this post.

    Each post has precisely one author.
    """
    author: User!
}
```
</details>

## Vertex types and properties

A [vertex][vertex] is akin to a table in SQL. Consider our Flutter example, 
where we have users and their posts, and also other users they follow. If we were
modelling it in SQL, we would have a `User` table and a `Post` table —
therefore in Trustfall we have a `User` vertex and a `Post` vertex.

A vertex can have properties associated with it (similar to columns in SQL), and
edges. We'll initially just focus on properties as these are the simplest case.
For a user we can imagine the following properties as a minimum:

* `url`: the URL to the users profile page (REQUIRED)
* `username`: the unique username of the account (REQUIRED)
* `display_name`: the name we display for the user (OPTIONAL)

With these two properties we can define a type for the schema called `User` which
looks as follows:

```graphql
"""
A user account on Flutter.
"""
type User {
    """
    The URL of this user's profile page.
    """
    url: String!

    """
    The user's unique username. All users are required to have one.
    """
    username: String!

    """
    The name the user would like to be known by, if any was set.

    If the user has not set a display name, this value will be `null`.
    """
    display_name: String
}
```

Here we've defined three string properties with doc comments to describe them. We've 
also made `url` and `username` required fields by adding the `!` suffix to the type. This
means that the value can't be null. Meanwhile, `display_name` is allowed to be
`null` as it is just a `String`.

We'll now also define our `Post` type: it will be very similar, with a url to the post and
a message representing the post. For now, the user who posted it will be omitted and covered
in the next section.

```graphql
"""
A message posted by a Flutter user.
"""
type Post implements Webpage {
    """
    The URL of this post.
    """
    url: String!

    """
    The contents of the posted message.
    """
    message: String!
}
```

## Edges

We may also want to query data that refers to other vertices. This is
what edges are for — relationships between other vertex types. For our user, these edges will
be the posts the user has created, and a list of the other users they follow.
We thus get the following schema for the `User` vertex type:

```graphql
"""
A user account on Flutter.
"""
type User {
    # own properties
    """
    The URL of this user's profile page.
    """
    url: String!

    """
    The user's unique username. All users are required to have one.
    """
    username: String!

    """
    The name the user would like to be known by, if any was set.

    If the user has not set a display name, this value will be `null`.
    """
    display_name: String
    # end of properties

    # own edges
    """
    The messages posted by this user, if any.
    """
    posts: [Post!]

    """
    The users this user follows, if any.
    """
    follows: [User!]
}
```

For readability, here we've split properties and edges using non-doc comments such as `# own edges`.

As a user can make multiple posts and follow multiple people, the type
of both edges is a list. Posts is a list of non-null post objects with the type `[Post!]` and
follows is similar a list of non-null users `[User!]`.

There are some nullability rules to be aware of with the Trustfall edges, these are all the valid
forms of a `Post` edge: 

* `Post` is 0 or 1 post
* `Post!` is exactly 1 post
* `[Post!]` is 0 or more posts
* `[Post!]!` is 1 or more posts

In our example the last one is the only one we won't be using. By making the list non-nullable `[Post!]!` 
it guarantees at least one value - whereas in normal GraphQL this can still be an empty list.
We also can't specify the type as `[Post]` as having a list of multiple posts that can be null doesn't
make sense — and would also complicate our queries.

Applying a similar change to `Post` we now add an edge for the author of the post, a single non-null
`User`.

```graphql
"""
A message posted by a Flutter user.
"""
type Post {
    # own properties
    """
    The URL of this post.
    """
    url: String!

    """
    The contents of the posted message.
    """
    message: String!

    # edges
    """
    The user who authored this post.

    Each post has precisely one author.
    """
    author: User!
}
```

## Interfaces

Both the User and Post vertex types have a url field, we can therefore have them both implement
the same interface. Interfaces are similar to GraphQL and programming languages which use them
in that we can define a set of properties an interface must provide and also set our edge and
query types to be interfaces instead of concrete types. Let's add a webpage interface to contain
the URL field and have our vertex types implement it:

```graphql
"""
A piece of data that has its own webpage.
"""
interface Webpage {
    # properties
    """
    The human-readable URL at which this webpage may be visited.
    """
    url: String!
}

"""
A user account on Flutter.
"""
type User implements Webpage {
    # properties from Webpage
    """
    The URL of this user's profile page.
    """
    url: String!

    # own properties
    """
    The user's unique username. All users are required to have one.
    """
    username: String!

    """
    The name the user would like to be known by, if any was set.

    If the user has not set a display name, this value will be `null`.
    """
    display_name: String
    # end of properties

    # own edges
    """
    The messages posted by this user, if any.
    """
    posts: [Post!]

    """
    The users this user follows, if any.
    """
    follows: [User!]
}

"""
A message posted by a Flutter user.
"""
type Post implements Webpage {
    # properties from Webpage
    """
    The URL of this post.
    """
    url: String!

    # own properties
    """
    The contents of the posted message.
    """
    message: String!

    # own edges
    """
    The user who authored this post.

    Each post has precisely one author.
    """
    author: User!
}
```

## Entrypoints

An entrypoint gives us an initial set of vertices to work with. Each Trustfall schema needs
a root element called `schema` with a field `query` that is our entrypoint type — the starting
point of the query.

In the entrypoint we want to provide the means to query our data source for the initial set of
vertices we'll consider and filter or transform. For data sources like APIs we'll usually define
some entrypoints that map to API endpoints. If all of our data is available without restrictions,
like in a SQL database, we might just define simple entrypoints named after
(and returning a list of) each vertex type. Although, even in that case we may want to
add some more sophisticated entrypoints, to simplify some operations that might
be complicated or impossible to express in a Trustfall query.

For this hypothetical Flutter app, we'll define an entrypoint type with two queries:

1. Find a user by their username
2. Search for posts with a given search query

Adding this into the schema would look like the following:

```graphql
schema {
    query: Entrypoints
}

type Entrypoints {
    """
    Find a specific user by their username, if they exist.
    """
    FindUser(username: String!): User

    """
    Run a search across all public posts with the given search terms.
    """
    SearchPosts(text: String!): [Post!]
}
```

Here our `FindUser` query takes a non-null string argument `username` and returns a `User`
which may or may not be null. The `SearchPosts` query takes a non-null string argument `text`
and returns a list of non-null `Post` where the list would be empty if there were no matching
posts.

[vertex]: ../glossary.md#vertex
[edge]: ../glossary.md#edge
[property]: ../glossary.md#property
[vertex-type]: ../glossary.md#vertex-type
[directive]: ../glossary.md#directive
