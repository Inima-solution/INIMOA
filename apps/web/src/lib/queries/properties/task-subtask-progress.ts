import { throwOnErr } from '@core/util/result';
import { propertiesServiceClient } from '@service-properties/client';
import type { GetTaskSubtaskProgress200Item } from '@service-properties/generated/schemas/getTaskSubtaskProgress200Item';
import {
  getTaskSubtaskProgressBody,
  getTaskSubtaskProgressResponse,
} from '@service-properties/generated/zod';
import { useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import { propertiesKeys } from './keys';

export async function fetchTaskSubtaskProgress(
  taskIds: readonly string[]
): Promise<GetTaskSubtaskProgress200Item[]> {
  const request = getTaskSubtaskProgressBody.parse({
    taskIds: [...taskIds],
  });
  const response = await throwOnErr(() =>
    propertiesServiceClient.getTaskSubtaskProgress({ body: request })
  );

  return getTaskSubtaskProgressResponse.parse(response);
}

export function useTaskSubtaskProgressQuery(
  taskIds: Accessor<readonly string[] | null | undefined>
) {
  return useQuery(() => {
    const currentTaskIds = taskIds();
    const ids = currentTaskIds ? [...currentTaskIds] : undefined;

    return {
      queryKey: ids
        ? propertiesKeys.taskSubtaskProgress(ids).queryKey
        : propertiesKeys.taskSubtaskProgress._def,
      queryFn: () => fetchTaskSubtaskProgress(ids!),
      enabled: ids !== undefined,
    };
  });
}
