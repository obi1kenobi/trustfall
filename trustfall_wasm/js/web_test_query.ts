import {Schema} from "../src/trustfall_wasm";

export default function testQuery() {
    const numbersSchema = Schema.parse(`
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) repeatable on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD
directive @transform(op: String!) on FIELD

type RootSchemaQuery {
    Number(min: Int = 0, max: Int!): [Number!]
    Zero: Number!
    One: Number!
    Two: Prime!
    Four: Composite!
}

interface Named {
    name: String
}

interface Number implements Named {
    name: String
    value: Int
    vowelsInName: [String]

    predecessor: Number
    successor: Number!
    multiple(max: Int!): [Composite!]
}

type Prime implements Number & Named {
    name: String
    value: Int
    vowelsInName: [String]

    predecessor: Number
    successor: Number!
    multiple(max: Int!): [Composite!]
}

type Composite implements Number & Named {
    name: String
    value: Int
    vowelsInName: [String]

    predecessor: Number
    successor: Number!
    multiple(max: Int!): [Composite!]
    divisor: [Number!]!
    primeFactor: [Prime!]!
}

type Letter implements Named {
    name: String
}
`);
}
