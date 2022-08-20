export type JsFieldValue = string | boolean | number | null | JsFieldValue[];

export class CrateInfo {}

/**
* @param {string} jsonText
* @returns {CrateInfo}
*/
export function makeCrateInfo(jsonText: string): CrateInfo;

/**
* @param {CrateInfo} crateInfo
* @param {string} query
* @param {Record<string, JsFieldValue>} args
* @returns {Record<string, JsFieldValue>[]}
*/
export function runQuery(
  crateInfo: CrateInfo,
  query: string,
  args: Record<string, JsFieldValue>,
): Record<string, JsFieldValue>[];

// WASM system initializer.
// Must be awaited before any of the other functionality is used.
export default function init(): Promise<any>;
