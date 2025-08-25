# `@filter` directive

The filter directive is a way to filter on properties in your Trustfall queries. This
means you can narrow down the vertices you iterate over based on various conditions.
A basic filter directive looks like the following:

```graphql
name @filter(op: "=", value: ["$name"])
```

For a list of vertex types, this filter would mean only vertices with the a name
equal to the supplied name would be yielded.

We can imagine this as equivalent to the following rust code:

```rust
vertices.iter()
    .filter(|x| x.name == name);
```

## Use with query variables

## Use with tagged values

### If the `@tag` comes from an `@optional` scope

## Operations

Filtering is restricted to allowed operations and these are as follows:

* Equality operators: `=` and `!=`
* Comparison operators: `<`, `<=`, `>` and `>=`
* Null-check operators: `is_null` and `is_not_null`
* Group membership operators: `one_of`, `not_one_of`, `contains` and `not_contains`
* String operators (each one has a `not_` version as well): `has_prefix`, `has_suffix`, `has_substring` and `regex`

## Equality operators: `=` and `!=`

## Comparison operators: `<`, `<=`, `>`, and `>=`

## Null-check operators: `is_null` and `is_not_null`

## Group membership operators

- `one_of` and `not_one_of`
- `contains` and `not_contains`

## String operators

- `has_prefix`, `not_has_prefix`, `has_suffix`, and `not_has_suffix`
- `has_substring` and `not_has_substring`
- `regex` and `not_regex`
