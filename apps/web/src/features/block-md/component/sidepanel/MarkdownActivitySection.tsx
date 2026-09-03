import { EntityActivitySectionConditional } from '@app/features/activity/EntityActivitySection';
import type { BlockAlias, BlockName } from '@core/block';
import type { EntityType } from '@service-properties/generated/schemas/entityType';

export function markdownActivityEntityType(
  blockName: BlockName | BlockAlias
): EntityType {
  return blockName === 'task' ? 'TASK' : 'DOCUMENT';
}

/** Keeps every Markdown subtype on the canonical exact-entity Activity path. */
export function MarkdownActivitySection(props: {
  blockId: string;
  blockName: BlockName | BlockAlias;
  order?: number;
}) {
  return (
    <EntityActivitySectionConditional
      entityId={props.blockId}
      entityType={markdownActivityEntityType(props.blockName)}
      order={props.order}
    />
  );
}
