import { throwOnErr } from '@core/util/result';
import {
  type BusinessAuditReauthenticationOutcome,
  type BusinessAuditRetentionFilter,
  businessAuditClient,
} from '@service-auth/business-audit';
import { useInfiniteQuery, useMutation, useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';

import { businessAuditKeys } from './keys';

export type {
  BusinessAuditReauthenticationOutcome,
  BusinessAuditRetentionFilter,
};

export function isBusinessAuditListEnabled(
  teamId: string | undefined,
  canRead: boolean | undefined
) {
  return Boolean(teamId) && canRead === true;
}

export function useBusinessAuditAccessQuery(
  teamId: Accessor<string | undefined>
) {
  return useQuery(() => {
    const currentTeamId = teamId();
    return {
      queryKey: businessAuditKeys.access(currentTeamId ?? '__none__').queryKey,
      queryFn: async () =>
        throwOnErr(async () => await businessAuditClient.getAccess()),
      enabled: Boolean(currentTeamId),
    };
  });
}

export function useBusinessAuditListQuery(input: {
  teamId: Accessor<string | undefined>;
  retentionClass: Accessor<BusinessAuditRetentionFilter | undefined>;
}) {
  const accessQuery = useBusinessAuditAccessQuery(input.teamId);
  return useInfiniteQuery(() => {
    const teamId = input.teamId();
    const retentionClass = input.retentionClass();
    return {
      queryKey: businessAuditKeys.list(teamId ?? '__none__', retentionClass)
        .queryKey,
      queryFn: async ({ pageParam }: { pageParam: string | undefined }) =>
        throwOnErr(
          async () =>
            await businessAuditClient.list({
              cursor: pageParam,
              retentionClass,
            })
        ),
      initialPageParam: undefined as string | undefined,
      getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
      enabled: isBusinessAuditListEnabled(teamId, accessQuery.data?.can_read),
    };
  });
}

export function useBusinessAuditDetailQuery(input: {
  teamId: Accessor<string | undefined>;
  eventId: Accessor<string | undefined>;
  enabled: Accessor<boolean>;
}) {
  return useQuery(() => {
    const teamId = input.teamId();
    const eventId = input.eventId();
    return {
      queryKey: businessAuditKeys.detail(
        teamId ?? '__none__',
        eventId ?? '__none__'
      ).queryKey,
      queryFn: async () =>
        throwOnErr(async () => await businessAuditClient.getDetail(eventId!)),
      enabled: Boolean(teamId) && Boolean(eventId) && input.enabled(),
    };
  });
}

export function useBusinessAuditPasswordReauthenticationMutation() {
  return useMutation(() => ({
    mutationFn: async (
      password: string
    ): Promise<BusinessAuditReauthenticationOutcome> =>
      throwOnErr(
        async () => await businessAuditClient.reauthenticatePassword(password)
      ),
  }));
}

export function useBusinessAuditMfaReauthenticationMutation() {
  return useMutation(() => ({
    mutationFn: async (input: {
      twoFactorId: string;
      code: string;
    }): Promise<BusinessAuditReauthenticationOutcome> =>
      throwOnErr(
        async () => await businessAuditClient.reauthenticateMfa(input)
      ),
  }));
}

export function useBusinessAuditExportMutation() {
  return useMutation(() => ({
    mutationFn: async (
      input: Parameters<typeof businessAuditClient.exportCsv>[0]
    ) => throwOnErr(async () => await businessAuditClient.exportCsv(input)),
  }));
}
