## `@fold` processing

Any time `@fold` is encountered, the result is defined to be equivalent to the following:
1. Cut the query edge `E` marked `@fold`, turning the original query into two queries: an outer query (the one that contains the original query root) and an inner query (the one that is to be folded). Assume the query edge `E` connects query vertex `A` to query vertex `B`.
2. Run the outer (query-root-containing) portion of the query.
3. Run the inner (to-be-folded) portion of the query.
4. For every query vertex `A` in the outer query, determine the instances of query edge `E` that connect it to a query vertex `B` in the inner query.
    a) If no such instances of `E`, then the fold outcome is an empty list of vertex sets.
    b) Otherwise, create a list of vertex sets by flat-mapping over the list of instances of `E`, looking up the instance's query vertex `B` assignment, and loading the inner query's result sets that start with the corresponding vertex.
5. Using this list of vertex sets for each query vertex `A` in the outer query:
    a) Process `_x_count` by referring to the size of the list.
    b) Process `@output` directives by getting the output field's value from each result set in the list.

### `@fold` inside `@fold`
The above process may be repeated recursively.

For a query with `@fold` inside `@fold`, the ultimate `@output` results on a field of type `X` will be `[[X]!]!` (non-null list of non-null list of `X`), where `X` may itself be nullable or non-nullable.

### `@fold` and `@recurse`

If the query edge `E` marked `@fold` is also marked `@recurse`, the usual `@fold` processing algorithm is used but with step 4. amended to include `@recurse` semantics.

Instances of query vertices `A` from the outer query and `B` from the inner query are considered connected by recursive query edge `E` if there is a path between the instances of `A` and `B` of between zero and recurse-depth instances of query edge `E`.

### `@fold` and `@tag`

Three situations worth considering:
- A `@fold` scope uses a `@tag` defined outside that `@fold`.
- A `@fold` scope uses a `@tag` defined within its own scope but not in a nested `@fold`.
- A scope (folded or not, doesn't matter) uses a `@tag` defined within a nested `@fold`.

TODO: Spec this out. Watch out for dependency cycles. "Tag is defined before being used" is a reasonable cycle-breaker; more sophisticated and relaxed rules may be used in the future.

## Parameterized edges

A parameterized edge is an edge that accepts parameters, as specified in the schema. These parameters are treated as a predicate that the edge must satisfy. The schema may specify that the parameter values are nullable or have default values (either null or non-null).

In the absence of edge directives like `@recurse` and `@optional`, the predicate specified by the edge parameters behaves analogously to a `@filter` directive -- consider the following hypothetical filesystem example:

```graphql
{
    Directory {
        name @output(out_name: "dir_name")

        out_Directory_ContainsFile(extension: "txt") {
            name @output(out_name: "file_name")
        }
    }
}
```

The above query would be semantically equivalent to the following query:
```graphql
{
    Directory {
        name @output(out_name: "dir_name")

        out_Directory_ContainsFile {
            name @output(out_name: "file_name")
            extension @filter(op_name: "=", value: ["$extension"])
        }
    }
}

{
    "extension": "txt"
}
```

However, when `@optional` or `@recurse` are used on those edges, the semantics between the parameterized edge approach and the `@filter` approach are no longer equivalent. Informally, this is because in the parameterized edge case, the predicate is part of the edge itself: `@optional` and `@recurse` will only consider edges as existing if they match the predicate. The following sections will expand on this definition.

### Parameterized edges marked `@optional`

Consider the following two hypothetical filesystem queries:
```graphql
{
    Directory {
        name @output(out_name: "dir_name")

        out_Directory_ContainsFile(extension: "txt") @optional {
            name @output(out_name: "file_name")
        }
    }
}
```
versus
```graphql
{
    Directory {
        name @output(out_name: "dir_name")

        out_Directory_ContainsFile @optional {
            extension @filter(op_name: "=", value: ["$extension"])
            name @output(out_name: "file_name")
        }
    }
}

{
    "extension": "txt"
}
```

The former query includes in its results all directories that contain only non-text files -- their results have a null `file_name` value.

The latter query does not include such directories in its results: the optional `out_Directory_ContainsFile` edge exists for such directories, but points to non-text files which get filtered out by the subsequent `@filter`.

### Parameterized edges marked `@optional`

Consider the following two hypothetical filesystem queries:
```graphql
{
    Directory {
        out_Directory_HasSubdirectory(modified_after: "2020-01-01") @recurse(depth: 10) {
            name @output(out_name: "subdirectory_name")
        }
    }
}
```
versus
```graphql
{
    Directory {
        name @output(out_name: "dir_name")

        out_Directory_HasSubdirectory @recurse(depth: 10) {
            last_modified @filter(op_name: ">", value: ["$after_date"])
            name @output(out_name: "subdirectory_name")
        }
    }
}

{
    "after_date": "2020-01-01"
}
```

The former query outputs directory names that are modified after 2020-01-01 where all their parent directories in the recursive traversal also satisfy that predicate. In the latter query, the recursion does not include that predicate -- only the final subdirectory in the recursion has to satisfy the predicate, and any intermediate subdirectories are not required to do so.

## Type coercions

Type coercion is semantically defined as a self-edge of at-most-one cardinality. As a result, type coercions may themselves be optional. For example:

```
{
    Directory {
        dir_name: name @output

        out_Directory_HasSubdirectory @recurse(depth: 10) {
            subdir_name: name @output

            out_Directory_File {
                file_name: name @output

                ... on TextFile @optional {
                    line_count @output
                }
            }
        }
    }
}
```
This query recurses into subdirectories, getting the names of their files, and also getting the line count for files that are in a textual format.

### Type coercions within `@optional` or `@fold` edges

The semantics of `@optional` with respect to `@filter` say that the edge's existence is unrelated to whether the vertex satisfies its property filters: if the `@optional` edge exists, processing for that vertex continues normally as if the edge weren't `@optional` at all.

Since type coercion is a filter-like operation (since its filtering effect can be equivalently expressed as a suitable `@filter` on the `__typename` property), consistency dictates that type coercion inside `@optional` behave the same way as filters. If the `@optional` edge exists but the resulting vertex cannot be coerced appropriately, its result set is discarded as if the edge were not `@optional`.
