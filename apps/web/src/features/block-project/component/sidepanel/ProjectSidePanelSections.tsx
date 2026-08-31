import { EntityActivitySectionConditional } from '@app/features/activity/EntityActivitySection';
import {
  EntityPropertiesSection,
  EntityTagsSection,
} from '@app/features/property/side-panel/properties';
import { SidePanel } from '@components/app/side-panel';
import { useBlockId } from '@core/block';
import { useCanEdit } from '@core/signal/permissions';
import { useBlockDocumentName } from '@core/util/currentBlockDocumentName';
import type { useProjectOverviewQuery } from '@queries/storage/project-overview';
import { Suspense } from 'solid-js';
import { ProjectOverviewSection } from './ProjectOverviewSection';

export function ProjectSidePanelSections(props: {
  query: ReturnType<typeof useProjectOverviewQuery>;
}) {
  const projectId = useBlockId();
  const canEdit = useCanEdit();
  const projectName = useBlockDocumentName();

  return (
    <>
      <ProjectOverviewSection order={5} query={props.query} />
      <SidePanel.Section id="details" title="Details" defaultOpen order={10}>
        <Suspense fallback={<SidePanel.Loading />}>
          <EntityPropertiesSection
            entityId={projectId}
            entityType="PROJECT"
            canEdit={canEdit()}
            documentName={projectName()}
            includeMetadata
            propertyFilter={(property) => property.isMetadata === true}
            showAddProperty={false}
            showTags={false}
          />
        </Suspense>
      </SidePanel.Section>
      <EntityTagsSection
        entityId={projectId}
        entityType="PROJECT"
        canEdit={canEdit()}
        order={20}
      />
      <SidePanel.Section
        id="properties"
        title="Properties"
        defaultOpen
        order={30}
      >
        <Suspense fallback={<SidePanel.Loading />}>
          <EntityPropertiesSection
            entityId={projectId}
            entityType="PROJECT"
            canEdit={canEdit()}
            documentName={projectName()}
            propertyFilter={(property) => property.isMetadata !== true}
            showTags={false}
          />
        </Suspense>
      </SidePanel.Section>
      <EntityActivitySectionConditional
        entityId={projectId}
        entityType="PROJECT"
        order={40}
      />
    </>
  );
}
