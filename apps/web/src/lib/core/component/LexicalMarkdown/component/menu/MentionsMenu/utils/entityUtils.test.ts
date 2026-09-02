import type { EntityItem } from '@core/context/quickAccess';
import type { DecisionEntity } from '@entity';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@core/constant/allBlocks', () => ({
  fileTypeToBlockName: (fileType: string) => fileType,
}));

import { getBlockNameFromEntity } from './entityUtils';

describe('getBlockNameFromEntity', () => {
  it('embeds a Decision as a generic collaborative markdown document mention', () => {
    const decision: EntityItem<DecisionEntity> = {
      kind: 'entity',
      bucket: 'decision',
      id: 'decision-1',
      searchText: 'Adopt event sourcing',
      sortTimestamp: 1,
      timestamps: {},
      data: {
        id: 'decision-1',
        name: 'Adopt event sourcing',
        ownerId: 'macro|owner@example.com',
        type: 'document',
        fileType: 'md',
        subType: { type: 'decision' },
        projectId: 'project-1',
      },
    };

    expect(getBlockNameFromEntity(decision)).toBe('md');
  });
});
