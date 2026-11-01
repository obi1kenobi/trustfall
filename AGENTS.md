# Repository guidance

## Tests and query text

Follow the [query test guidance](trustfall_core/test_data/README.md#writing-and-reviewing-test-queries)
when writing or reviewing queries. Inspect query text in both Rust source and
`.graphql.ron` test files, including its formatting, readability, and semantics.
Rust formatting checks do not cover the embedded GraphQL.

Use explicit imports in new test modules. Do not introduce `use super::*`.

## Adversarial review

Use a fresh independent reviewer for every review round, including follow-up rounds.
The reviewer must not have implemented or designed the change.

Review the full change, including new files, comments, test queries, and generated
fixtures. Challenge unnecessary code and test machinery. Check that comments
explain the code's requirements without relying on discussion outside the repository.

For each meaningful condition or exclusion, identify tests for both the excluded
case and a nearby valid case that must remain supported. Check duplicated
implementations independently; coverage of one does not establish coverage of another.

Report which checks actually ran and what they cover. Inspect embedded query text
directly before claiming its formatting has been checked.
