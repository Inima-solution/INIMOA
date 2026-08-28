import { throwOnErr } from '@core/util/result';
import { storageServiceClient } from '@service-storage/client';
import type { GetProjectTaskDependencyReadiness200Item } from '@service-storage/generated/schemas';
import { getProjectTaskDependencyReadinessBody } from '@service-storage/generated/zod';
import { useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import { entityKeys } from './keys';

export async function fetchProjectTaskDependencyReadiness(
  projectId: string,
  taskIds: readonly string[]
): Promise<GetProjectTaskDependencyReadiness200Item[]> {
  const request = getProjectTaskDependencyReadinessBody.parse({
    taskIds: [...taskIds],
  });

  return throwOnErr(() =>
    storageServiceClient.projects.getTaskDependencyReadiness({
      id: projectId,
      taskIds: request.taskIds,
    })
  );
}

export function useProjectTaskDependencyReadinessQuery(
  projectId: Accessor<string | null | undefined>,
  taskIds: Accessor<readonly string[]>
) {
  return useQuery(() => {
    const id = projectId();
    const ids = [...taskIds()];

    return {
      queryKey: id
        ? entityKeys.projectTaskDependencyReadiness(id, ids).queryKey
        : entityKeys.projectTaskDependencyReadiness._def,
      queryFn: () => fetchProjectTaskDependencyReadiness(id!, ids),
      enabled: !!id,
    };
  });
}
