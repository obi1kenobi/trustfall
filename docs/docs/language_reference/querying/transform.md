# `@transform` directive

The transform directive allows you apply a transformation to a value where
the `op` argument defines the transformation. Currently, only a single op
`count` is supported.

Using the schema in the [Trustfall schema guide](../schema/index.md), we
could filter out stories with fewer than a given number of comments as follows:

```graphql
comment @fold @transform(op: "count") @filter(op: ">", value: ["$threshold"]) {
    textPlain @output
}
```

The transform directive is typically used like this, coupled with a fold and filter
directive on a list of items.

By applying an output directive, we can also output the count from our query.

```graphql
query {
    Top(max: 20) {
        ... on Story {
            title @output
            comment @fold @transform(op: "count") @output(name: "comment_count") {
                textPlain
            }
        }
    }
}
```

The field inside the folded scope tells Trustfall which traversed vertices should
be counted. In this case, it counts comments on each story.
