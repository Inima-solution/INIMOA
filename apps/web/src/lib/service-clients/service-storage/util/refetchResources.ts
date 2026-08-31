import { refetchDocumentShareButtonResource } from '@core/component/TopBar/ShareButton';
import { invalidateUserQuota } from '@queries/auth';
import { refetchHistory } from '@queries/history/history';
import { invalidatePreview } from '@queries/preview';
import { invalidateDeletedItems } from '@queries/storage/deleted';
import { invalidateProjectOverviews } from '@queries/storage/project-overview';
import { invalidateProjects } from '@queries/storage/projects';

export function refetchResources() {
  // TODO: fetch documents
  // refetchDocuments();
  invalidateUserQuota();
  refetchHistory();
  refetchProjectResources();
  invalidateDeletedItems();
  invalidatePreview();
}

async function refetchProjectResources(_force = false) {
  await Promise.all([invalidateProjects(), invalidateProjectOverviews()]);

  refetchDocumentShareButtonResource();
}
