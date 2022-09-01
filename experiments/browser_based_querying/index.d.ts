// Tell Typescript to interpret imports of .example files as strings
declare module '*.example' {
  const content: string;
  export default content;
}

// Tell Typescript to interpret imports of .graphql files as strings
declare module '*.graphql' {
  const content: string;
  export default content;
}
