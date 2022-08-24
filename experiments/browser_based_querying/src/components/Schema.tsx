import { buildSchema } from "graphql";

// import { HN_SCHEMA } from '../adapter'; / * Ignore for now -- test adding documentation

// TODO: This is a copy of schema.graphql, find a better way to include it.
export const HN_SCHEMA = `
schema {
  query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD

type RootSchemaQuery {
  HackerNewsFrontPage: [HackerNewsItem!]!
  HackerNewsTop(max: Int): [HackerNewsItem!]!
  HackerNewsLatestStories(max: Int): [HackerNewsStory!]!
  HackerNewsUser(name: String!): HackerNewsUser
}

interface HackerNewsItem {
  id: Int!
  unixTime: Int!
  ownUrl: String!
}

type HackerNewsJob implements HackerNewsItem {
  # properties from HackerNewsItem
  id: Int!
  unixTime: Int!
  ownUrl: String!

  # own properties
  title: String!
  score: Int!
  url: String!

  # edges
  link: Webpage!
}

"""A hacker news story."""
type HackerNewsStory implements HackerNewsItem {
  # properties from HackerNewsItem
  id: Int!
  unixTime: Int!
  ownUrl: String!

  # own properties
  byUsername: String!
  score: Int!
  text: String
  title: String!
  url: String
  commentsCount: Int!

  # edges
  byUser: HackerNewsUser!
  comment: [HackerNewsComment!]
  link: Webpage
}

"""A comment left on a hacker news story."""
type HackerNewsComment implements HackerNewsItem {
  # properties from HackerNewsItem
  id: Int!
  unixTime: Int!
  ownUrl: String!

  # own properties
  text: String!
  byUsername: String!

  # edges
  byUser: HackerNewsUser!
  reply: [HackerNewsComment!]
  "Either a parent comment or the story being commented on"
  parent: HackerNewsItem!  

  # not implemented yet
  # topmostAncestor: HackerNewsItem!  # the ultimate ancestor of this item: a story or job
}

"""Information about a hacker news user."""
type HackerNewsUser {
  id: String!
  karma: Int!
  about: String
  unixCreatedAt: Int!

  """ The HackerNews API treats submissions of comments and stories the same way.
  The way to get only a user's submitted stories is to use this edge then
  apply a type coercion on the \`HackerNewsItem\` vertex on edge endpoint:
  \`...on HackerNewsStory\`"""
  submitted: [HackerNewsItem!]
}

"""Information about a webpage on the Internet."""
interface Webpage {
  url: String!
}
`;

const GRAPHQL_SCHEMA = buildSchema(HN_SCHEMA);
export default GRAPHQL_SCHEMA;