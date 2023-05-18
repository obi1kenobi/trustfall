export type JsFieldValue = string | boolean | number | null | JsFieldValue[];
export type JsEdgeParameters = Record<string, JsFieldValue>;

export interface JsContext<T> {
    free(): void;

    readonly localId: number;
    readonly activeVertex: T | null;
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
    resolveStartingVertices(
        edge: string,
        parameters: JsEdgeParameters,
    ): IterableIterator<T>;

    resolveProperty(
        contexts: IterableIterator<JsContext<T>>,
        type_name: string,
        field_name: string
    ): IterableIterator<ContextAndValue>;

    resolveNeighbors(
        contexts: IterableIterator<JsContext<T>>,
        type_name: string,
        edge_name: string,
        parameters: JsEdgeParameters,
    ): IterableIterator<ContextAndNeighborsIterator<T>>;

    resolveCoercion(
        contexts: IterableIterator<JsContext<T>>,
        type_name: string,
        coerce_to_type: string
    ): IterableIterator<ContextAndBool>;
}

export class Schema {
    free(): void;
    /**
     * Returns a Set<string> which contains every subtype of type_name
     * in the schema and itself. This allows you to get the subtypes using
     * this function, then in resolveCoercion return whether the set returned
     * by this function has the typename string of the vertex from the iterator,
     * which would mean coercible from it's current form to the form wanted by
     * resolveCoercion.
     * @param {string} type_name
     * @returns {Set<string>}
     */
    subtypes(type_name: string): Set<string>

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

export function initialize(): void;
