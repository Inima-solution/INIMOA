import type { QuickAccessItem } from '@core/context/quickAccess/types';
import { describe, expect, it } from 'vitest';
import { indexAvailableTaskEntities } from './task-entity-index';

const person = (id: string): QuickAccessItem =>
  ({
    id,
    kind: 'user',
    bucket: 'person',
    data: { id, name: 'Ada', email: `${id}@example.test` },
  }) as QuickAccessItem;

describe('task entity refinement index', () => {
  it('treats an unmaterialized passwordless-login list as empty', () => {
    expect(indexAvailableTaskEntities(undefined)).toEqual(new Map());
  });

  it('ignores transient holes while retaining materialized entities', () => {
    const item = person('user-1');
    expect(indexAvailableTaskEntities([undefined, item])).toEqual(
      new Map([['user-1', item]])
    );
  });
});
