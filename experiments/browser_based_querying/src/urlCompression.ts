// Decoding tables:
// "-" => " "
// "--_0" thru "--_9", then "--_a" thru "--_z", then "--_A" thru "--_Z" => 2+ spaces, run-length encoded
// "--0" thru "--9", then "--a" thru "--z", then "--A" thru "--Z" => "{\n" followed by 2+ spaces, run-length encoded
// "--*0" thru "--*9", then "--*a" thru "--*z", then "--*A" thru "--*Z" => "}\n" followed by 2+ spaces, run-length encoded
// N.B.: "---" is a not a legal codeword because it means "space followed by '--' escape sequence"
//
// "*l" => "\n"
// "*L" => ","
// "**" => '"'
// "*-" => "-"
// "*c" => "["
// "*j" => "]"
// "*C" => "{"
// "*J" => "}"
// "*B" => ":"
// "*1" => "!"
// "*2" => "@"
// "*3" => "#"
// "*4" => "$"
// "*5" => "%"
// "*8" => "*"
// "*9" => "("
// "*0" => ")"
// "*g" => "<"
// "*G" => ">"
// "*e" => "="
// "*o" => "@output"
// "*O" => "@optional"
// "*t" => "@tag"
// "*f" => "@filter"
// "*T" => "@transform"
// "*F" => "@fold"
// "*r" => "@recurse"
// "*E" => "... on "
// "*n" => "name:"
// "*d" => "depth:"
// "*v" => "value:"
// "*p" => "op:"

function invertAndStripPrefix(data: Record<string, string>, prefix: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const key in data) {
    let value = data[key];
    if (value.startsWith(prefix)) {
      value = value.substring(prefix.length);
    }
    result[value] = key;
  }
  return result;
}

const DIRECTIVE_REPLACEMENTS: Record<string, string> = {
  '@output': '*o',
  '@optional': '*O',
  '@tag': '*t',
  '@transform': '*T',
  '@filter': '*f',
  '@fold': '*F',
  '@recurse': '*r',
}

const SPECIAL_CHAR_REPLACEMENTS: Record<string, string> = {
  '"': '**',
  '-': '*-',
  '[': '*c',
  ']': '*j',
  '{': '*C',
  '}': '*J',
  ':': '*B',
  '!': '*1',
  '@': '*2',
  '#': '*3',
  '$': '*4',
  '%': '*5',
  '*': '*8',
  '(': '*9',
  ')': '*0',
  '\n': '*l',
  ',': '*L',
  '<': '*g',
  '>': '*G',
  '=': '*e',
}

const DICTIONARY_REPLACEMENTS: Record<string, string> = {
  '... on': '*E',
  'name:': '*n',
  'depth:': '*d',
  'value:': '*v',
  'op:': '*p',
}

function makeDecodingBook(): Record<string, string> {
  const inv_special = invertAndStripPrefix(SPECIAL_CHAR_REPLACEMENTS, '*');
  const inv_directive = invertAndStripPrefix(DIRECTIVE_REPLACEMENTS, '*');
  const inv_dict = invertAndStripPrefix(DICTIONARY_REPLACEMENTS, '*');

  return {
    ...inv_special,
    ...inv_directive,
    ...inv_dict
  };
}

const DECODING_BOOK = makeDecodingBook();

function encodeRunLength(substr: string): [string, number] {
  let offset = 0;
  let runLength = 0;
  let result = '';

  let openCurly = substr.startsWith('{\n');
  let closeCurly = substr.startsWith('}\n');
  if (openCurly || closeCurly) {
    offset += 2;
    substr = substr.substring(2);
  } else if (substr.startsWith('{')) {
    return [SPECIAL_CHAR_REPLACEMENTS['{'], 0];
  } else if (substr.startsWith('}')) {
    return [SPECIAL_CHAR_REPLACEMENTS['}'], 0];
  }

  while (runLength < substr.length && substr.charAt(runLength) == ' ') {
    runLength++;
  }

  if (runLength == 0) {
    if (openCurly) {
      return [SPECIAL_CHAR_REPLACEMENTS['{'] + SPECIAL_CHAR_REPLACEMENTS['\n'], 1];
    } else if (closeCurly) {
      return [SPECIAL_CHAR_REPLACEMENTS['}'] + SPECIAL_CHAR_REPLACEMENTS['\n'], 1];
    } else {
      // This branch should be unreachable.
      throw new Error(`unreachable: runLength = 0 without curly braces: ${substr}`);
    }
  } else if (runLength == 1) {
    if (openCurly) {
      return [SPECIAL_CHAR_REPLACEMENTS['{'] + SPECIAL_CHAR_REPLACEMENTS['\n'] + '-', 2];
    } else if (closeCurly) {
      return [SPECIAL_CHAR_REPLACEMENTS['}'] + SPECIAL_CHAR_REPLACEMENTS['\n'] + '-', 2];
    } else {
      return ['-', 0];
    }
  }

  let remaining = runLength;
  while (remaining > 0) {
    result += '-';
    if (remaining == 1) {
      break;
    }

    result += '-';

    if (closeCurly) {
      result += '*';
      closeCurly = false;
    } else if (openCurly) {
      openCurly = false;
    } else {
      result += '_';
    }

    remaining -= 2;
    if (remaining < 10) {
      result += String.fromCharCode('0'.charCodeAt(0) + remaining);
      break;
    }
    remaining -= 10;

    if (remaining < 26) {
      result += String.fromCharCode('a'.charCodeAt(0) + remaining);
      break;
    }
    remaining -= 26;

    const next = Math.min(remaining, 25);
    remaining -= next;
    result += String.fromCharCode('A'.charCodeAt(0) + next);
  }

  return [result, offset + runLength - 1];
}

export function compress(str: string): string {
  let result = '';
  for (let i = 0; i < str.length; i++) {
    const c = str.charAt(i);
    if (c == ' ' || c == '{' || c == '}') {
      const substr = str.substring(i);
      const [encoded, advance] = encodeRunLength(substr);
      result += encoded;
      i += advance;
    } else {
      // Directives are super common and long, use a custom escape sequence for them.
      if (c == '@') {
        const remainder = str.substring(i, i + 10);  // grab a substring longer than any directive
        let matched = false;
        for (const directive in DIRECTIVE_REPLACEMENTS) {
          if (remainder.startsWith(directive)) {
            const replacement = DIRECTIVE_REPLACEMENTS[directive];
            result += replacement;
            i += directive.length - 1;

            matched = true;
            break;
          }
        }

        if (matched) {
          continue;
        }
      }

      // Attempt to match against our dictionary of syntactic phrases.
      let dict_matched = false;
      for (const item in DICTIONARY_REPLACEMENTS) {
        // Avoid string splicing if first char is not a match for this dictionary entry.
        if (item.startsWith(c)) {
          // First char matched, check the full entry.
          if (item == str.substring(i, i + item.length)) {
            const code = DICTIONARY_REPLACEMENTS[item];
            result += code;
            i += item.length - 1;

            dict_matched = true;
            break;
          }
        }
      }
      if (dict_matched) {
        continue;
      }

      // Some characters require URI escaping, which is a minimum of 3 chars.
      // See if we can save some chars by using a custom escape sequence.
      if (c in SPECIAL_CHAR_REPLACEMENTS) {
        result += SPECIAL_CHAR_REPLACEMENTS[c];
        continue;
      }

      // No luck! Represent the character as itself.
      result += c;
    }
  }

  return result;
}

const zero = '0'.charCodeAt(0);
const nine = '9'.charCodeAt(0);
const lower_a = 'a'.charCodeAt(0);
const lower_z = 'z'.charCodeAt(0);
const upper_a = 'A'.charCodeAt(0);
const upper_z = 'Z'.charCodeAt(0);

function decodeRunLength(value: string): string | null {
  let result = '  ';
  let extraRepetitions = 0;

  const code = value.charCodeAt(0);
  if (code >= zero && code <= nine) {
    extraRepetitions += code - zero;
  } else {
    extraRepetitions += 10;

    if (code >= lower_a && code <= lower_z) {
      extraRepetitions += code - lower_a;
    } else {
      extraRepetitions += 26;

      if (code >= upper_a && code <= upper_z) {
        extraRepetitions += code - upper_a;
      } else {
        // Unexpected character after escape sequence. The input is corrupted.
        return null;
      }
    }
  }

  while (extraRepetitions--) {
    result += ' ';
  }

  return result;
}

export function decompress(str: string): string | null {
  let result = '';

  for (let i = 0; i < str.length; i++) {
    const c = str.charAt(i);

    if (c == '-') {
      if (i == str.length || str.charAt(i + 1) != '-') {
        result += ' ';
        continue;
      }

      i += 2;
      if (i == str.length) {
        // Escape sequence not followed by any character. Input is corrupted.
        return null;
      }

      let next = str.charAt(i);
      if (next == '-') {
        // We found '---', this is actually a space followed by an escape sequence.
        result += ' ';
        i -= 2;
        continue;
      } else if (next == '*') {
        result += '}\n';

        i++;
        if (i == str.length) {
          // Escape sequence not followed by any character. Input is corrupted.
          return null;
        }
        next = str.charAt(i);
      } else if (next == '_') {
        i++;
        if (i == str.length) {
          // Escape sequence not followed by any character. Input is corrupted.
          return null;
        }
        next = str.charAt(i);
      } else {
        result += '{\n';
      }

      const run = decodeRunLength(next);
      if (run == null) {
        // Corrupted input.
        console.error("corrupted run length:", next);
        return null;
      }
      result += run;
    } else if (c == '*') {
      i++;
      if (i == str.length) {
        // No symbol after '*', the input is corrupted.
        return null;
      }

      const next = str.charAt(i);
      if (next in DECODING_BOOK) {
        result += DECODING_BOOK[next];
      } else {
        // Unexpected character after escape sequence. The input is corrupted.
        console.error("unexpected char after escape:", next);
        return null;
      }
    } else {
      result += c;
    }
  }

  return result;
}
