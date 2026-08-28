import { throwOnErr } from '@core/util/result';
import { queryClient } from '@queries/client';
import { storageServiceClient } from '@service-storage/client';
import type {
  ProjectOperations,
  ReplaceProjectOperationsRequest,
} from '@service-storage/generated/schemas';
import { useMutation, useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import { entityKeys } from './keys';

type ReplaceProjectOperationsInput = {
  projectId: string;
  request: ReplaceProjectOperationsRequest;
};

export async function fetchProjectOperations(
  projectId: string
): Promise<ProjectOperations> {
  return throwOnErr(() =>
    storageServiceClient.projects.getOperations({ id: projectId })
  );
}

export function useProjectOperationsQuery(
  projectId: Accessor<string | null | undefined>
) {
  return useQuery(() => {
    const id = projectId();
    return {
      queryKey: id
        ? entityKeys.projectOperations(id).queryKey
        : entityKeys.projectOperations._def,
      queryFn: () => fetchProjectOperations(id!),
      enabled: !!id,
    };
  });
}

export function useReplaceProjectOperationsMutation() {
  return useMutation(() => ({
    mutationFn: async ({ projectId, request }: ReplaceProjectOperationsInput) =>
      throwOnErr(() =>
        storageServiceClient.projects.replaceOperations({
          id: projectId,
          ...request,
        })
      ),
    onSuccess: (operations, { projectId }) => {
      queryClient.setQueryData(
        entityKeys.projectOperations(projectId).queryKey,
        operations
      );
      queryClient.invalidateQueries({
        queryKey: entityKeys.projectOverview(projectId).queryKey,
        exact: true,
      });
    },
  }));
}
