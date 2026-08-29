import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import {
  createSoupState,
  type SoupState,
} from '@app/features/next-soup/create-soup-state';
import { defineQueryFilters } from '@app/features/next-soup/filters/filter-store';
import { soupItemMatchesProjectMembership } from '@app/features/next-soup/filters/query-filters';
import { SoupContextProvider } from '@app/features/next-soup/soup-context';
import { SoupViewList } from '@app/features/next-soup/soup-view/soup-view';
import {
  SoupViewContextProvider,
  useSoupView,
} from '@app/features/next-soup/soup-view/soup-view-context';
import {
  openEntityInSplitFromUnifiedList,
  preventDuplicatePreviewEntityOpen,
} from '@app/features/next-soup/utils';
import { getIsSpecialProject } from '@block-project/isSpecial';
import { SidePanel } from '@components/app/side-panel';
import { useEntryState } from '@components/app/split-layout/entry-state';
import { useSplitPanelOrThrow } from '@components/app/split-layout/layoutUtils';
import { useBlockId } from '@core/block';
import { DocumentBlockContainer } from '@core/component/DocumentBlockContainer';
import { FileDropOverlay } from '@core/component/FileDropOverlay';
import { toast } from '@core/component/Toast/Toast';
import { fileFolderDrop } from '@core/directive/fileFolderDrop';
import { fileSelector } from '@core/directive/fileSelector';
import { blockHotkeyScopeSignal } from '@core/signal/blockElement';
import { useCanEdit } from '@core/signal/permissions';
import {
  handleFileFolderDrop,
  type UploadInput,
  uploadFiles,
} from '@core/util/upload';
import type { TaskEntityWithProperties } from '@entity';
import { isTaskEntity } from '@entity/types/entity';
import { getTaskStatusOptionId } from '@entity/utils/task-properties';
import { SYSTEM_PROPERTY_IDS } from '@property/constants';
import { useAllProperties } from '@property/editor/hooks/useAllProperties';
import { TaskDependencyRelationsProvider } from '@property/task-dependency-relations';
import { TaskSubtaskProgressProvider } from '@property/task-subtask-progress';
import type { Property } from '@property/types';
import { useBulkSaveEntityPropertiesMutation } from '@queries/properties/entity';
import { refetchSoupEntity } from '@queries/soup/cache';
import { EntityType } from '@service-properties/generated/schemas/entityType';
import { refetchResources } from '@service-storage/util/refetchResources';
import { type Component, createMemo, createSignal, Show } from 'solid-js';
import { ModalsProvider } from './ModalsProvider';
import { ProjectTaskStatusBoard } from './ProjectTaskStatusBoard';
import type { ProjectTaskViewMode } from './ProjectViewModeControl';
import { ProjectSidePanelSections } from './sidepanel/ProjectSidePanelSections';
import { TopBar } from './TopBar';

// HACK: prevent lint error on custom directive
false && fileFolderDrop;
false && fileSelector;

const PROJECT_ENTITY_TYPES = ['document', 'task', 'chat', 'project', 'email'];

const Block: Component = () => {
  useBlockEntityCommands();
  const [isDragging, setIsDragging] = createSignal(false);
  const projectId = useBlockId();
  const isSpecialProject = getIsSpecialProject(projectId);
  const [taskViewMode, setTaskViewMode] = useEntryState<ProjectTaskViewMode>(
    'project.taskViewMode',
    { default: 'list' }
  );
  const viewMode = () => (isSpecialProject ? 'list' : taskViewMode());

  const handleFileUpload = async (files: UploadInput[]) => {
    if (files.length === 0) return;

    // Don't allow uploads to root or trash
    if (isSpecialProject) {
      toast.failure('Cannot upload files to this location');
      return;
    }

    try {
      const results = await uploadFiles(files, 'dss', {
        projectId,
      });

      const uploads = results.filter((result) => !result.failed);

      // refetch successfully uploaded documents into soup
      const successfulUploads = uploads.filter((result) => !result.pending);
      for (const upload of successfulUploads) {
        if (upload.type === 'document') {
          refetchSoupEntity(upload.documentId, 'document');
        }
      }
      if (successfulUploads.length > 0) {
        refetchResources();
      }

      // wait for pending folder uploads to finish upload before refetching resources
      const pendingFolderUploads = uploads
        .filter((result) => result.pending)
        .filter((result) => result.type === 'folder')
        .map((result) => result.projectId);
      if (pendingFolderUploads.length > 0) {
        const resolved = await Promise.all(pendingFolderUploads);
        for (const projectId of resolved) {
          if (projectId) {
            refetchSoupEntity(projectId, 'project');
          }
        }
        refetchResources();
      }
    } catch (error) {
      console.error('Upload error:', error);
      toast.failure('Upload failed. Please try again.');
    }
  };

  const projectSoup = createSoupState({
    initialPredicates: { and: ['project-content'] },
    predicateConfigs: [
      {
        id: 'project-content',
        predicate: (entity: { type: string }) =>
          PROJECT_ENTITY_TYPES.includes(entity.type),
      },
    ],
  });

  return (
    <DocumentBlockContainer>
      <div
        class="size-full bg-surface flex flex-col relative"
        use:fileFolderDrop={{
          onDragStart: () => setIsDragging(true),
          onDragEnd: () => setIsDragging(false),
          onDrop: (fileEntries, folderEntries) => {
            handleFileFolderDrop(fileEntries, folderEntries, handleFileUpload);
          },
          disabled: isSpecialProject,
        }}
      >
        <ModalsProvider>
          <Show when={isDragging() && !isSpecialProject}>
            <FileDropOverlay>Upload to this folder</FileDropOverlay>
          </Show>
          <SidePanel.Layout defaultOpen={false}>
            <Show when={!isSpecialProject}>
              <ProjectSidePanelSections />
            </Show>
            <div class="flex size-full min-w-0 flex-col overflow-hidden">
              <TopBar
                mode={viewMode()}
                onChange={setTaskViewMode}
                selectorVisible={!isSpecialProject}
              />
              <ProjectEntityList
                mode={viewMode()}
                projectId={projectId}
                soup={projectSoup}
                // Scope is already attached by the block container so we can use that
                // Change this when we remove blocks
                scopeId={blockHotkeyScopeSignal.get()}
              />
            </div>
          </SidePanel.Layout>
        </ModalsProvider>
      </div>
    </DocumentBlockContainer>
  );
};

const ProjectEntityList = (props: {
  mode: ProjectTaskViewMode;
  scopeId: string;
  projectId: string;
  soup: SoupState;
}) => {
  return (
    <SoupContextProvider soup={props.soup}>
      <SoupViewContextProvider
        soup={props.soup}
        initialEnabled
        itemMembershipFilter={
          getIsSpecialProject(props.projectId)
            ? undefined
            : (item) => soupItemMatchesProjectMembership(item, props.projectId)
        }
        initialQuery={defineQueryFilters({
          include: {
            // Filter documents by project
            projectId: [props.projectId],
            // Filter chats by project
            chatProjectId: [props.projectId],
            // Filter projects by project (current project only)
            folderId: [props.projectId],
            // Filter emails by project
            emailProjectId: [props.projectId],
          },
          // Default 'inbox' view would hide archived/outbound-only threads
          emailView: 'all',
        })}
      >
        <ProjectSoupViewList
          isSpecialProject={getIsSpecialProject(props.projectId)}
          mode={props.mode}
          scopeId={props.scopeId}
        />
      </SoupViewContextProvider>
    </SoupContextProvider>
  );
};

const ProjectSoupViewList = (props: {
  isSpecialProject: boolean;
  mode: ProjectTaskViewMode;
  scopeId: string;
}) => {
  const soupView = useSoupView();
  const { searchText, source, soup } = soupView;
  const panel = useSplitPanelOrThrow();
  const canEdit = useCanEdit();
  const allProperties = useAllProperties();
  const [activeStatusTaskId, setActiveStatusTaskId] = createSignal<string>();
  const restoreStatusControlFocus = (
    taskId: string,
    onComplete: () => void
  ) => {
    const controlId = encodeURIComponent(taskId);
    requestAnimationFrame(() => {
      const control = document.querySelector(
        `[data-project-task-status-control="${controlId}"]`
      );
      if (control instanceof HTMLSelectElement && control.isConnected) {
        control.focus();
      }
      onComplete();
    });
  };
  const statusMutation = useBulkSaveEntityPropertiesMutation({
    onSettled: () => {
      const taskId = activeStatusTaskId();
      if (taskId) {
        restoreStatusControlFocus(taskId, () =>
          setActiveStatusTaskId(undefined)
        );
      } else {
        setActiveStatusTaskId(undefined);
      }
    },
  });
  const statusProperty = createMemo<Property | undefined>(() => {
    const definition = allProperties().find(
      (property) => property.id === SYSTEM_PROPERTY_IDS.STATUS
    );
    if (!definition || definition.valueType !== 'SELECT_STRING') return;
    return {
      propertyId: definition.id,
      propertyDefinitionId: definition.id,
      displayName: definition.displayName,
      isMultiSelect: definition.isMultiSelect,
      isMetadata: definition.isMetadata,
      isSystemProperty: definition.isSystem,
      isRequired: true,
      options: definition.options,
      owner: definition.owner,
      specificEntityType: definition.specificEntityType,
      createdAt: definition.createdAt,
      updatedAt: definition.updatedAt,
      valueType: 'SELECT_STRING',
      value: null,
    };
  });
  const taskIds = createMemo(() => {
    if (props.isSpecialProject) return [];

    return soupView.rows().flatMap((row) => {
      if (row.getIsGrouped() || row.getIsLoadMore()) return [];
      return isTaskEntity(row.original) ? [row.original.id] : [];
    });
  });
  const tasks = () =>
    props.isSpecialProject
      ? []
      : (source.data().filter(isTaskEntity) as TaskEntityWithProperties[]);
  const boardLoading = () =>
    tasks().length === 0 &&
    (source.isLoading() || source.isPlaceholderData() || source.isFetching());
  const retryBoard = () => {
    void source.refresh().catch(() => {});
  };
  const openTask = (task: TaskEntityWithProperties, event: MouseEvent) => {
    if (
      !event.shiftKey &&
      !event.altKey &&
      panel.handle.isControllerSplit() &&
      preventDuplicatePreviewEntityOpen(task, panel.handle)
    ) {
      return;
    }

    soup.focus.set(task.id);
    void openEntityInSplitFromUnifiedList(task, {
      openInNewSplit: event.shiftKey,
      replacePreview: !event.shiftKey && event.altKey,
      splitHandle: panel.handle,
      referredFrom: null,
    });
  };
  const moveTaskStatus = (task: TaskEntityWithProperties, statusId: string) => {
    const property = statusProperty();
    if (
      !canEdit() ||
      !property ||
      statusMutation.isPending ||
      getTaskStatusOptionId(task) === statusId
    ) {
      return;
    }
    setActiveStatusTaskId(task.id);
    statusMutation.mutate({
      properties: [
        {
          entityId: task.id,
          entityType: EntityType.TASK,
          property,
          apiValues: { valueType: 'SELECT_STRING', values: [statusId] },
        },
      ],
    });
  };

  return (
    <TaskSubtaskProgressProvider taskIds={taskIds}>
      <TaskDependencyRelationsProvider taskIds={taskIds}>
        <Show
          when={!props.isSpecialProject && props.mode === 'board'}
          fallback={
            <SoupViewList
              customScrollbarHidden={true}
              scopeId={props.scopeId}
            />
          }
        >
          <ProjectTaskStatusBoard
            tasks={tasks()}
            loading={boardLoading()}
            error={source.error() !== null}
            searching={searchText().trim().length > 0}
            onOpenTask={openTask}
            onRetry={retryBoard}
            canEdit={canEdit()}
            statusProperty={statusProperty()}
            statusPending={statusMutation.isPending}
            activeStatusTaskId={activeStatusTaskId()}
            onMoveTaskStatus={moveTaskStatus}
          />
        </Show>
      </TaskDependencyRelationsProvider>
    </TaskSubtaskProgressProvider>
  );
};

export default Block;
