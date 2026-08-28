import { createQueryKeys } from '@lukemorales/query-key-factory';

export const businessAuditKeys = createQueryKeys('businessAudit', {
  access: (teamId: string) => ({ queryKey: [teamId, 'access'] }),
  list: (teamId: string, retentionClass: string | undefined) => ({
    queryKey: [teamId, 'list', retentionClass ?? '__all__'],
  }),
  detail: (teamId: string, eventId: string) => ({
    queryKey: [teamId, 'detail', eventId],
  }),
});
