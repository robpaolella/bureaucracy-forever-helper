// A small observable state. The window renders from one of these and redraws when it changes.
export interface Store<T> {
  get(): T;
  /** Replaces the state with `update(current)` and tells every subscriber. */
  set(update: (current: T) => T): void;
  /** Calls `listener` after every change; returns a function that stops it. */
  subscribe(listener: () => void): () => void;
}

export function createStore<T>(initial: T): Store<T> {
  let state = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => state,
    set(update) {
      state = update(state);
      listeners.forEach((listener) => listener());
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
