import { throwOnErr } from '@core/util/result';
import { storageServiceClient } from '@service-storage/client';
import type { GetProjectOverview200DataOneOf } from '@service-storage/generated/schemas';
import { useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import { entityKeys } from './keys';

export async function fetchProjectOverview(
  projectId: string
): Promise<GetProjectOverview200DataOneOf> {
  return throwOnErr(() =>
    storageServiceClient.projects.getOverview({ id: projectId })
  );
}

export function useProjectOverviewQuery(
  projectId: Accessor<string | null | undefined>
) {
  return useQuery(() => {
    const id = projectId();
    return {
      queryKey: id
        ? entityKeys.projectOverview(id).queryKey
        : entityKeys.projectOverview._def,
      queryFn: () => fetchProjectOverview(id!),
      enabled: !!id,
    };
  });
}
