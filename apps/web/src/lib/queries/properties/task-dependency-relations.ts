import { throwOnErr } from '@core/util/result';
import { propertiesServiceClient } from '@service-properties/client';
import type { GetTaskDependencyRelations200Item } from '@service-properties/generated/schemas/getTaskDependencyRelations200Item';
import {
  getTaskDependencyRelationsBody,
  getTaskDependencyRelationsResponse,
} from '@service-properties/generated/zod';
import { useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import { propertiesKeys } from './keys';

export type TaskDependencyRelations = Pick<
  GetTaskDependencyRelations200Item,
  | 'taskId'
  | 'readiness'
  | 'dependsOnTaskIds'
  | 'blockingTaskIds'
  | 'hasUnavailableDependencies'
  | 'successorTaskIds'
  | 'hasUnavailableSuccessors'
>;

export async function fetchTaskDependencyRelations(
  taskIds: readonly string[]
): Promise<TaskDependencyRelations[]> {
  const request = getTaskDependencyRelationsBody.parse({
    taskIds: [...taskIds],
  });
  const response = await throwOnErr(() =>
    propertiesServiceClient.getTaskDependencyRelations({ body: request })
  );
  const relations = getTaskDependencyRelationsResponse.parse(response);

  return relations.map((relation) => ({
    taskId: relation.taskId,
    readiness: relation.readiness,
    dependsOnTaskIds: relation.dependsOnTaskIds,
    blockingTaskIds: relation.blockingTaskIds,
    hasUnavailableDependencies: relation.hasUnavailableDependencies,
    successorTaskIds: relation.successorTaskIds,
    hasUnavailableSuccessors: relation.hasUnavailableSuccessors,
  }));
}

export function useTaskDependencyRelationsQuery(
  taskIds: Accessor<readonly string[] | null | undefined>
) {
  return useQuery(() => {
    const currentTaskIds = taskIds();
    const ids = currentTaskIds ? [...currentTaskIds] : undefined;

    return {
      queryKey: ids
        ? propertiesKeys.taskDependencyRelations(ids).queryKey
        : propertiesKeys.taskDependencyRelations._def,
      queryFn: () => fetchTaskDependencyRelations(ids!),
      enabled: ids !== undefined,
    };
  });
}
