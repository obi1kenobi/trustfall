# Trustfall schemas

Datasets in Trustfall are described using a [schema](../glossary.md#schema). The schema describes the shape of each data point in the dataset, and how different data points are connected to each other.

The schema is used to validate and type-check queries, making sure they are valid before attempting to execute them. It also powers editor conveniences such as code auto-complete.

## Schema components overview

In Trustfall, all datasets are modelled as a graph, consisting of [typed][vertex-type] [vertices][vertex] containing [properties][property], and connected to each other by [edges][edge].

Since editor auto-complete also relies on schema data, Trustfall [directives][directive] are included in each Trustfall schema. Their definitions specify the query positions where each directive is valid and may be suggested as a code auto-completion. The schema directives section is always the same for any given Trustfall version, and must not be changed by hand.

<!-- the below is more of a guide than a reference -->

### Example setup

This guide uses the HackerNews playground schema as its running example. HackerNews is a useful example because the data is familiar and available through public APIs: users submit stories, stories have comments, and comments may have replies.

The live playground can run queries against this schema in your browser:
[open the HackerNews playground](https://play.predr.ag/hackernews).

<details>
<summary>
If you'd like to peek ahead, click here to reveal the small schema subset we'll build up to.
</summary>

```graphql
schema {
    query: RootSchemaQuery
}

type RootSchemaQuery {
    """
    The top items on HackerNews.
    """
    Top(max: Int): [Item!]!

    """
    Latest story submissions on HackerNews.
    """
    Latest(max: Int): [Story!]!

    """
    Look up a user by their username.
    """
    User(name: String!): User
}

"""
A data item with its own webpage.
"""
interface Webpage {
    """
    The URL where this item may be viewed.
    """
    url: String!
}

"""
One of the kinds of items on HackerNews: a story, job, comment, etc.
"""
interface Item implements Webpage {
    """
    The item's URL on HackerNews.
    """
    url: String!

    """
    The item's unique identifier.
    """
    id: Int!
}

"""
A HackerNews user account.
"""
type User implements Webpage {
    """
    The user's HackerNews profile URL.
    """
    url: String!

    """
    The user's HackerNews username.
    """
    id: String!

    """
    The user's current karma score.
    """
    karma: Int!

    """
    Items submitted by this user.
    """
    submitted: [Item!]
}

"""
A story submitted to HackerNews.
"""
type Story implements Item & Webpage {
    """
    The story's URL on HackerNews.
    """
    url: String!

    """
    The story's title.
    """
    title: String!

    """
    The current score of this story submission.
    """
    score: Int!

    """
    The URL submitted by this story, if any.
    """
    submittedUrl: String

    """
    The user who submitted the story.
    """
    byUser: User!

    """
    Top-level comments on this story.
    """
    comment: [Comment!]
}

"""
A comment on HackerNews.
"""
type Comment implements Item & Webpage {
    """
    The comment's URL on HackerNews.
    """
    url: String!

    """
    The comment text with HTML removed.
    """
    textPlain: String!

    """
    The user who wrote the comment.
    """
    byUser: User!

    """
    Replies to this comment.
    """
    reply: [Comment!]
}
```
</details>

## Vertex types and properties

A [vertex][vertex] is akin to a table in SQL. In the HackerNews schema, `User`, `Story`, and `Comment` are vertex types. Each vertex type describes one kind of data item that queries can inspect.

A vertex can have properties associated with it, similar to columns in SQL, and edges to other vertices. We'll initially just focus on properties as these are the simplest case. For a HackerNews story, useful properties include:

* `url`: the story's URL on HackerNews (REQUIRED)
* `title`: the story's title (REQUIRED)
* `score`: the story's current score (REQUIRED)
* `submittedUrl`: the URL submitted by the story, if any (OPTIONAL)

With these properties, a `Story` type can be defined as follows:

```graphql
"""
A story submitted to HackerNews.
"""
type Story {
    """
    The story's URL on HackerNews.
    """
    url: String!

    """
    The story's title.
    """
    title: String!

    """
    The current score of this story submission.
    """
    score: Int!

    """
    The URL submitted by this story, if any.
    """
    submittedUrl: String
}
```

Here we've defined three required properties and one optional property. The `!` suffix means the value can't be null. Since text submissions like "Ask HN" do not always have an external submitted URL, `submittedUrl` is allowed to be `null`.

We'll now also define a `User` type. HackerNews users have a profile URL, a username, and a karma score:

```graphql
"""
A HackerNews user account.
"""
type User {
    """
    The user's HackerNews profile URL.
    """
    url: String!

    """
    The user's HackerNews username.
    """
    id: String!

    """
    The user's current karma score.
    """
    karma: Int!
}
```

## Edges

We may also want to query data that refers to other vertices. This is what edges are for: relationships between vertex types. For a HackerNews story, useful edges include the user who submitted it and the comments posted on it.

Adding those edges to `Story` looks like this:

```graphql
"""
A story submitted to HackerNews.
"""
type Story {
    # own properties
    """
    The story's URL on HackerNews.
    """
    url: String!

    """
    The story's title.
    """
    title: String!

    """
    The current score of this story submission.
    """
    score: Int!

    """
    The URL submitted by this story, if any.
    """
    submittedUrl: String
    # end of properties

    # edges
    """
    The user who submitted the story.
    """
    byUser: User!

    """
    Top-level comments on this story.
    """
    comment: [Comment!]
}
```

For readability, here we've split properties and edges using non-doc comments such as `# edges`.

The `byUser` edge is non-nullable because each story has a submitter. The `comment` edge is a list because each story may have zero or more comments.

There are some nullability rules to be aware of with Trustfall edges. These are all the valid forms of a `Comment` edge:

* `Comment` is 0 or 1 comment
* `Comment!` is exactly 1 comment
* `[Comment!]` is 0 or more comments
* `[Comment!]!` is 1 or more comments

By making the list non-nullable, `[Comment!]!` guarantees at least one value. That is different from normal GraphQL, where a non-null list may still be empty. We also can't specify the type as `[Comment]`, because having a list of multiple comments that can be null doesn't make sense and would complicate queries.

Applying the same idea to `User`, we can add an edge for all items submitted by that user:

```graphql
"""
A HackerNews user account.
"""
type User {
    # own properties
    """
    The user's HackerNews profile URL.
    """
    url: String!

    """
    The user's HackerNews username.
    """
    id: String!

    """
    The user's current karma score.
    """
    karma: Int!
    # end of properties

    # edges
    """
    Items submitted by this user.
    """
    submitted: [Item!]
}
```

## Interfaces

Several HackerNews vertex types have a `url` property, so they can implement a common interface. Interfaces are similar to GraphQL and programming language interfaces: they define a set of properties an implementing type must provide, and they allow edges and entrypoints to return an interface instead of a concrete type.

For example, the HackerNews schema has a `Webpage` interface that both `User` and `Story` can implement:

```graphql
"""
A data item with its own webpage.
"""
interface Webpage {
    # properties
    """
    The URL where this item may be viewed.
    """
    url: String!
}

"""
A HackerNews user account.
"""
type User implements Webpage {
    # properties from Webpage
    """
    The user's HackerNews profile URL.
    """
    url: String!

    # own properties
    """
    The user's HackerNews username.
    """
    id: String!

    """
    The user's current karma score.
    """
    karma: Int!
}

"""
A story submitted to HackerNews.
"""
type Story implements Webpage {
    # properties from Webpage
    """
    The story's URL on HackerNews.
    """
    url: String!

    # own properties
    """
    The story's title.
    """
    title: String!

    """
    The current score of this story submission.
    """
    score: Int!
}
```

The full HackerNews playground schema also has an `Item` interface. `Story`, `Comment`, and `Job` all implement it, which lets the `Top(max:)` entrypoint return a mixed list of HackerNews items while queries choose the concrete item kinds they care about.

## Entrypoints

An entrypoint gives us an initial set of vertices to work with. Each Trustfall schema needs a root element called `schema` with a field `query` that is our entrypoint type: the starting point of the query.

In the entrypoint we want to provide the means to query our data source for the initial set of vertices we'll consider and filter or transform. For data sources like APIs, entrypoints usually map to API endpoints or other efficient lookup operations.

For the HackerNews playground, useful entrypoints include:

1. List top HackerNews items.
2. List the latest story submissions.
3. Find a user by username.

Adding this into the schema would look like the following:

```graphql
schema {
    query: RootSchemaQuery
}

type RootSchemaQuery {
    """
    The top items on HackerNews.
    """
    Top(max: Int): [Item!]!

    """
    Latest story submissions on HackerNews.
    """
    Latest(max: Int): [Story!]!

    """
    Look up a user by their username.
    """
    User(name: String!): User
}
```

Here `Top(max:)` takes an optional integer argument and returns a list of `Item` vertices. Since `Item` is an interface, a query can use a type coercion such as `... on Story` to keep only stories and discard jobs or other item kinds. The `User(name:)` entrypoint takes a non-null string argument and returns a `User` if one exists.

## Runnable example queries

The following query gets the first 20 top HackerNews stories and outputs their title, score, submitted URL, HackerNews URL, submitter username, and submitter karma.

```graphql
query {
  Top(max: 20) {
    ... on Story {
      title @output
      score @output
      storyUrl: url @output
      submittedUrl @output

      byUser {
        submitter: id @output
        karma @output
      }
    }
  }
}
```

Variables:

```json
{

}
```

[Run this query in the HackerNews playground](https://play.predr.ag/hackernews#?f=2&q=query---0Top*9max*B-20*0---2*E-Story---4title-*o*l--_4score-*o*l--_4storyUrl*B-url-*o*l--_4submittedUrl-*o*l*l--_4byUser---6submitter*B-id-*o*l--_6karma-*o*l--_4--*2--*0*J*l*J&v=*C*l*l*J)

This next query shows how variables are passed into filters. It looks at the latest 100 stories, keeps only those with a minimum score, then follows the `byUser` edge and keeps only stories whose submitter has enough karma.

```graphql
query {
  Latest(max: 100) {
    title @output
    byUsername @output
    submittedUrl @output
    score @output @filter(op: ">=", value: ["$minScore"])
    storyUrl: url @output

    byUser {
      karma @output @filter(op: ">=", value: ["$minKarma"])
    }
  }
}
```

Variables:

```json
{
  "minScore": 5,
  "minKarma": 300
}
```

[Run this query in the HackerNews playground](https://play.predr.ag/hackernews#?f=2&q=query---0Latest*9max*B-100*0---2title-*o*l--_2byUsername-*o*l--_2submittedUrl-*o*l--_2score-*o-*f*9*p-***G*e***L-*v-*c***4minScore***j*0*l--_2storyUrl*B-url-*o*l*l--_2byUser---4karma-*o-*f*9*p-***G*e***L-*v-*c***4minKarma***j*0*l--_2--*0*J*l*J&v=--0**minScore***B-5*L*l--_0**minKarma***B-300*l*J)

[vertex]: ../glossary.md#vertex
[edge]: ../glossary.md#edge
[property]: ../glossary.md#property
[vertex-type]: ../glossary.md#vertex-type
[directive]: ../glossary.md#directive
