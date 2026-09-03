import type { QuickAccessItem } from '@core/context/quickAccess/types';

/**
 * Quick Access is Suspense-backed and can briefly expose no materialized list
 * while a passwordless login replaces the unauthenticated provider state.
 * Filter refinements are optional UI and must render as empty during that
 * handoff rather than taking down the whole app error boundary.
 */
export function indexAvailableTaskEntities(
  items: readonly (QuickAccessItem | undefined)[] | undefined
): Map<string, QuickAccessItem> {
  const byId = new Map<string, QuickAccessItem>();
  for (const item of items ?? []) {
    if (item) byId.set(item.id, item);
  }
  return byId;
}
