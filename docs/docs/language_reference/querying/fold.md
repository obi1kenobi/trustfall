# `@fold` directive

The fold directive is applied to edges. It aggregates the traversed results across that edge into lists.

For example:

```graphql
# in a query, at type `Story`
comment @fold {
    textPlain @output
}
```

This would return _a list_ of the text of that story's top-level comments.
