"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.testQuery = void 0;
const trustfall_wasm_1 = require("../src/trustfall_wasm");
function testQuery() {
    const numbersSchema = trustfall_wasm_1.Schema.parse(`
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
exports.testQuery = testQuery;
