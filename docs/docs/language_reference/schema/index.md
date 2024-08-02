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

## Edges

## Entrypoints

## Interfaces


In the Flutter data model:
- A specific user of Flutter would be represented as a [vertex][vertex].
- All user vertices together represent the schema's `User` [vertex type][vertex-type].
- Each user has a username — that's a [property][property] of the `User` vertex type.
- Users may follow each other's activity: the schema models that as the `follows` [edge][edge] on the `User` [vertex type][vertex-type].
- If user `A` follows other users `X` and `Y`, we say that vertex `A` has `follows` edges to `X` and `Y`. A given user may have zero, one, or more `follows` edges.

[vertex]: ../glossary.md#vertex
[edge]: ../glossary.md#edge
[property]: ../glossary.md#property
[vertex-type]: ../glossary.md#vertex-type
[directive]: ../glossary.md#directive
