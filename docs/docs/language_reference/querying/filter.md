# `@filter` directive

The filter directive is a way to filter on properties in your Trustfall queries. This
means you can narrow down the vertices you iterate over based on various conditions.
Filters can have arguments, but the simplest ones are have no arguments. An example
of this would be:

```graphql
name @filter(op: "is_not_null")
```

Here we would only select vertices where the `name` property isn't `null`. Naturally,
there's a limited number of useful filters where the only input is the property in
question. For the other operations where the property is compared against some input
you need to either provide a query variable or a tagged value. Currently,
literal values like `3` or `"hi"` are not valid arguments into a filter directive
— they have to be passed in via variables instead.

Filters don't have logical operators. Instead, applying multiple `@filter` directives
to a property will implicitly apply all their conditions together, as if joined with `AND`.
Below is a toy example using the null checking filters, which is guaranteed to yield zero entries.

```graphql
name @filter(op: "is_not_null") @filter(op: "is_null")
```

## Use with query variables

A query variable is a value passed in from outside the query that is prefixed with `$`.
Below is an example of filtering for equality on a name using a variable `name` provided
at execution time:

```graphql
name @filter(op: "=", value: ["$name"])
```

For a list of vertex types, this filter would mean only vertices with the name
equal to the supplied name would be yielded.

We can imagine this as equivalent to the following Rust code:

```rust
vertices.iter()
    .filter(|x| x.name == name);
```

## Use with tagged values

Tagged values allow us to filter using values found within the query itself.

Using the schema in the [Trustfall schema guide](../schema/index.md), if we
want to find users whose username is equal to their display name,
we can write:

```graphql
username @tag
display_name @filter(op: "=", value: ["%username"])
```

### If the `@tag` comes from an `@optional` scope

An `@optional` directive can be applied to an edge and then values inside that edge
be tagged for use in filters. Filters using these tags are only evaluated if the edge
existed, if the edge wasn't present the filter expression is true.

## Operations

Filtering is restricted to allowed operations and these are as follows:

* Equality operators: `=` and `!=`
* Comparison operators: `<`, `<=`, `>` and `>=`
* Null-check operators: `is_null` and `is_not_null`
* Group membership operators: `one_of`, `not_one_of`, `contains` and `not_contains`
* String operators (each one has a `not_` version as well): `has_prefix`, `has_suffix`, `has_substring` and `regex`

The following subsections contain more details about these operations.

## Equality operators

- `=` and `!=`

These are fairly standard equals and not-equals operators you'll be familiar with
from other languages.

## Comparison operators

- `<`, `<=`, `>`, and `>=`

The comparison operators should be familiar from other languages, like in SQL comparison
to `null` is `false`.

## Null-check operators: `is_null` and `is_not_null`

These operators take no arguments and should only be applied to nullable properties.

## Group membership operators

- `one_of` and `not_one_of`
- `contains` and `not_contains`

The `one_of` and `not_one_of` operators require the value provided to the filter is a non-nullable list
of values. Constrastingly, the `contains` and `not_contains` operators require that the property is
a list so the elements of the list can be checked for equality against the argument.

## String operators

- `has_prefix`, `not_has_prefix`, `has_suffix`, and `not_has_suffix`
- `has_substring` and `not_has_substring`
- `regex` and `not_regex`

The string operators require that the provided value is non-null.
