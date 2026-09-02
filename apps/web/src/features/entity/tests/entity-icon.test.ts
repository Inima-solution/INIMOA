import { describe, expect, it } from 'vitest';
import { getEntityIconType } from '../extractors/entity-icon-type';

describe('getEntityIconType', () => {
  it('keeps a Decision distinct from a generic markdown document', () => {
    expect(
      getEntityIconType({
        id: 'decision-1',
        name: 'Architecture choice',
        ownerId: 'user-1',
        type: 'document',
        fileType: 'md',
        subType: { type: 'decision' },
        projectId: 'project-1',
      })
    ).toBe('decision');
  });
});
