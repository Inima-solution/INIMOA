import { throwOnErr } from '@core/util/result';
import { queryClient } from '@queries/client';
import { storageServiceClient } from '@service-storage/client';
import type { GetProjectOverview200DataOneOf } from '@service-storage/generated/schemas';
import { useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import { entityKeys } from './keys';

export async function fetchProjectOverview(
  projectId: string,
  asOfDate: string = localAsOfDate()
): Promise<GetProjectOverview200DataOneOf> {
  return throwOnErr(() =>
    storageServiceClient.projects.getOverview({ id: projectId, asOfDate })
  );
}

function localAsOfDate(now = new Date()): string {
  return [now.getFullYear(), now.getMonth() + 1, now.getDate()]
    .map((part, index) =>
      index === 0 ? String(part) : String(part).padStart(2, '0')
    )
    .join('-');
}

/** Mark one project's overview, or every cached project overview, stale. */
export function invalidateProjectOverviews(projectId?: string): Promise<void> {
  return queryClient.invalidateQueries({
    queryKey: projectId
      ? entityKeys.projectOverview(projectId).queryKey
      : entityKeys.projectOverview._def,
    exact: false,
  });
}

export function useProjectOverviewQuery(
  projectId: Accessor<string | null | undefined>
) {
  return useQuery(() => {
    const id = projectId();
    const asOfDate = localAsOfDate();
    return {
      queryKey: id
        ? [...entityKeys.projectOverview(id).queryKey, asOfDate]
        : entityKeys.projectOverview._def,
      queryFn: () => fetchProjectOverview(id!, asOfDate),
      enabled: !!id,
    };
  });
}
