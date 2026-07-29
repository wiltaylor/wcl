/* Node ≥22 ships an experimental global `localStorage` that evaluates to
   `undefined` unless the process gets `--localstorage-file`, and its getter
   shadows the happy-dom environment's storage. Give the suite a real
   in-memory Storage so code (and tests) using localStorage behave as in a
   browser. */

if (globalThis.localStorage == null) {
  const store = new Map();
  const stub = {
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
    getItem: (k) => (store.has(String(k)) ? store.get(String(k)) : null),
    setItem: (k, v) => store.set(String(k), String(v)),
    removeItem: (k) => store.delete(String(k)),
    key: (i) => [...store.keys()][i] ?? null,
  };
  Object.defineProperty(globalThis, 'localStorage', { value: stub, configurable: true });
}
