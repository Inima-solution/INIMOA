/** Prefixes one Soup provider's split-history key without changing legacy keys. */
export function soupViewEntryStateKey(key: string, namespace?: string): string {
  return namespace ? `${namespace}.${key}` : key;
}

/**
 * Resolves one provider's initial state. A namespaced provider cannot consume
 * an ordinary provider's history snapshot (or the reverse), while a snapshot
 * from its own namespace retains the existing highest priority.
 */
export function selectSoupViewInitialValue<T>(args: {
  entryState?: Record<string, unknown>;
  key: string;
  namespace?: string;
  initialValue?: T;
  persistedValue?: T;
  preferInitial?: boolean;
}): T | undefined {
  const entryValue = args.entryState?.[
    soupViewEntryStateKey(args.key, args.namespace)
  ] as T | null | undefined;

  return (
    entryValue ??
    (args.preferInitial ? args.initialValue : undefined) ??
    args.persistedValue ??
    args.initialValue
  );
}
