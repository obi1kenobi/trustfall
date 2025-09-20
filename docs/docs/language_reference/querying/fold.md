# `@fold` directive

The fold directive is applied to edges. It aggregates the traversed results across that edge into lists.

For example:
```
# in a query, at type `User`
follower @fold {
    display_name @output
}
```
This would return _a list_ of the display names of that user's followers.
