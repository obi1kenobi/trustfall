type JsFieldValue = string | boolean | number | null | JsFieldValue[];
type JsEdgeParameters = Record<string, JsFieldValue>;

interface JsContext<T> {
  local_id: number;
  value: T | null;
}

interface ContextAndValue {
  local_id: number;
  value: JsFieldValue;
}

interface ContextAndNeighborsIterator<T> {
  local_id: number;
  neighbors: IterableIterator<T>;
}

interface ContextAndBool {
  local_id: number;
  value: boolean;
}

interface JsAdapter<T> {
  get_starting_tokens(
    edge: string,
    parameters: JsEdgeParameters | null
  ): IterableIterator<T>;

  project_property(
    data_contexts: IterableIterator<JsContext<T>>,
    current_type_name: string,
    field_name: string
  ): IterableIterator<ContextAndValue>;

  project_neighbors(
    data_contexts: IterableIterator<JsContext<T>>,
    current_type_name: string,
    edge_name: string,
    parameters: JsEdgeParameters | null
  ): IterableIterator<ContextAndNeighborsIterator<T>>;

  can_coerce_to_type(
    data_contexts: IterableIterator<JsContext<T>>,
    current_type_name: string,
    coerce_to_type_name: string
  ): IterableIterator<ContextAndBool>;
}
