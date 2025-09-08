# `@transform` directive

The transform directive allows you apply a transformation to a value where
the `op` argument defines the transformation. Currently, only a single op
`count` is supported.

Using the schema in the [Trustfall schema guide](../schema/index.md) we
could filter out authors posts posts with less than a given number of followers
as so:

```graphql
author {
    follows @fold @transform(op: "count") filter(op: ">", value: ["$followThreshold]) {
        # extract information we want or apply other operations
    }
}
```

The transform directive is typically used like this, coupled with a fold and filter 
directive on a list of items.

By applying an output directive, we can also output the count from our query.

```graphql
query {
    FindUser(username: "predrag") 
    posts @fold @transform(op: "count") @output(name: "post_count") 
}
```
