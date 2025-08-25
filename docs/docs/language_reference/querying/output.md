# `@output` directive

## Implicit output name

## Aliases

## Assigning aliases to a scope

### Nesting aliased scopes

### Implicit vs explicit output names

`alias: value @output` uses an implicit output name. The final output name here will consist of all prefixes from nested aliased scopes, followed by `alias`.

`value @output` also uses an implicit output name. It is semantically equivalent to `value: value @output`, as if the chosen alias name is the same as the property's own name.

`value @output(name: "my_output")` is an explicit output name. The output name here is `my_output`, regardless of any prefixes or aliases.
