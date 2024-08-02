# `@optional` directive

## What if the edge exists but the other vertex doesn't match its `@filter`

The `@optional` directive will not pretend that it "didn't see" the edge. As soon as the edge is determined to exist for the given vertex, the remainder of the query proceeds as if no `@optional` was present.
