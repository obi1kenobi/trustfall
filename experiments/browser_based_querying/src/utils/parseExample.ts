/**
 * Parse an example file into query and vars
 */
export default function parseExample(exampleFile: string): [string, string] {
  const lines = exampleFile.split('\n');
  let query = '';
  let vars = '';

  let section: "query" | "vars" = "query";
  lines.forEach(line => {
    if (section === "query") {
      if (!line.startsWith("vars:")) {
        query = `${query}${line}\n`
      } else {
        section = "vars";
      }
    } else {
      vars = `${vars}${line}\n`
    }
  })

  return [query.trim(), vars.trim()];
}
