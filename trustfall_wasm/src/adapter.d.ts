export type JsFieldValue = string | boolean | number | null | JsFieldValue[];
export type JsEdgeParameters = Record<string, JsFieldValue>;

export interface JsContext<T> {
    free(): void;

    readonly localId: number;
    readonly currentToken: T | null;
}

export interface ContextAndValue {
    localId: number;
    value: JsFieldValue;
}

export interface ContextAndNeighborsIterator<T> {
    localId: number;
    neighbors: IterableIterator<T>;
}

export interface ContextAndBool {
    localId: number;
    value: boolean;
}

export interface Adapter<T> {
    getStartingTokens(
        edge: string,
        parameters: JsEdgeParameters,
    ): IterableIterator<T>;

    projectProperty(
        data_contexts: IterableIterator<JsContext<T>>,
        current_type_name: string,
        field_name: string
    ): IterableIterator<ContextAndValue>;

    projectNeighbors(
        data_contexts: IterableIterator<JsContext<T>>,
        current_type_name: string,
        edge_name: string,
        parameters: JsEdgeParameters,
    ): IterableIterator<ContextAndNeighborsIterator<T>>;

    canCoerceToType(
        data_contexts: IterableIterator<JsContext<T>>,
        current_type_name: string,
        coerce_to_type_name: string
    ): IterableIterator<ContextAndBool>;
}

export class Schema {
    free(): void;

    /**
    * @param {string} input
    * @returns {Schema}
    */
    static parse(input: string): Schema;
}

/**
* @param {Schema} schema
* @param {Adapter<T>} adapter
* @param {string} query
* @param {Record<string, JsFieldValue>} args
* @returns {IterableIterator<Record<string, JsFieldValue>>}
*/
export function executeQuery<T>(
    schema: Schema,
    adapter: Adapter<T>,
    query: string,
    args: Record<string, JsFieldValue>,
): IterableIterator<Record<string, JsFieldValue>>;
