import { match } from 'ts-pattern';
import type { EntityData } from '../types/entity';
import {
  isCallEntity,
  isChannelEntity,
  isChannelMessageEntity,
  isDecisionEntity,
  isSkillEntity,
  isSnippetEntity,
  isTaskEntity,
} from '../types/entity';

/** Resolve the icon key used for an entity row without rendering UI. */
export function getEntityIconType(entity: EntityData) {
  return (
    match(entity)
      .when(isChannelEntity, ({ channelType }) => channelType)
      .when(isChannelMessageEntity, ({ channelType }) => channelType)
      .when(isTaskEntity, () => 'task')
      .when(isSnippetEntity, () => 'snippet')
      .when(isSkillEntity, () => 'skill')
      .when(isDecisionEntity, () => 'decision')
      .with({ type: 'document' }, ({ fileType }) => fileType ?? 'default')
      .with({ type: 'chat' }, () => 'chat')
      .with({ type: 'project' }, () => 'project')
      .with({ type: 'email' }, ({ isRead, hasIcsAttachment }) =>
        hasIcsAttachment ? 'emailInvite' : isRead ? 'emailRead' : 'email'
      )
      .when(isCallEntity, () => 'call')
      .with({ type: 'automation' }, () => 'automation')
      .with(
        { type: 'foreign', foreignSource: 'github_pull_request' },
        () => 'githubPullRequest'
      )
      .with({ type: 'foreign' }, () => 'default')
      .with({ type: 'crm_company' }, () => 'crm_company')
      // Always the bell, never the referenced entity's icon: the row is a
      // reminder first, and what it points at is iconed beside its name.
      .with({ type: 'reminder' }, () => 'reminder')
      .otherwise(() => 'default')
  );
}
