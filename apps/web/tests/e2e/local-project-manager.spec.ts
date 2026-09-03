import { execFileSync } from 'node:child_process';

import { expect, type Locator, test } from '@playwright/test';

import { entityIdSelector } from '../../src/lib/core/dom-selectors';
import {
  expectEntityInCurrentList,
  gotoApp,
  LOCAL_E2E,
  uniqueE2EText,
} from './helpers/local-app';

const STATUS_PROPERTY_ID = '00000001-0000-0000-0000-000000000002';
const PARENT_TASK_PROPERTY_ID = '00000001-0000-0000-0000-000000000005';
const SUBTASKS_PROPERTY_ID = '00000001-0000-0000-0000-000000000006';
const DEPENDS_ON_PROPERTY_ID = '00000001-0000-0000-0000-000000000007';
const MILESTONE_PROPERTY_ID = '00000001-0000-0000-0000-000000000013';
const START_DATE_PROPERTY_ID = '00000001-0000-0000-0000-000000000014';
const DUE_DATE_PROPERTY_ID = '00000001-0000-0000-0000-000000000004';
const NOT_STARTED_STATUS_ID = '00000001-0000-0000-0002-000000000001';
const COMPLETED_STATUS_ID = '00000001-0000-0000-0002-000000000004';

test.skip(!LOCAL_E2E, 'requires LOCAL_E2E=true and the isolated smoke seed');

test('plans and monitors one canonical project across every manager view', async ({
  page,
}) => {
  test.setTimeout(300_000);
  const token = tokenFor('manager@inimoa.local');
  const objective = uniqueE2EText('WS-05 manager objective');
  const milestoneName = uniqueE2EText('WS-05 release milestone');
  const childName = uniqueE2EText('WS-05 completed subtask');
  const decisionName = uniqueE2EText('WS-05 launch decision');
  let projectId: string | undefined;

  try {
    await gotoApp(page, '/component/documents');
    const createResponse = page.waitForResponse(
      (response) =>
        response.request().method() === 'POST' &&
        new URL(response.url()).pathname.endsWith('/dss/projects')
    );
    await page.keyboard.press('c');
    await expect(page.getByText('Create New', { exact: true })).toBeVisible();
    await page.keyboard.press('f');
    const created = await createResponse;
    expect(created.status()).toBe(200);
    projectId = projectIdFrom(await created.json());
    await expect(page).toHaveURL(new RegExp(projectId));

    await replaceOperations(projectId, token, {
      objective,
      startDate: dateOnly(0),
      targetDate: dateOnly(14),
    });
    const milestoneId = await createTask(projectId, milestoneName, token, [
      selectProperty(STATUS_PROPERTY_ID, NOT_STARTED_STATUS_ID),
      {
        propertyId: MILESTONE_PROPERTY_ID,
        value: { type: 'boolean', value: true },
      },
      dateProperty(START_DATE_PROPERTY_ID, 0),
      dateProperty(DUE_DATE_PROPERTY_ID, 7),
    ]);
    const childId = await createTask(projectId, childName, token, [
      selectProperty(STATUS_PROPERTY_ID, NOT_STARTED_STATUS_ID),
      dateProperty(DUE_DATE_PROPERTY_ID, 3),
    ]);
    await clearTaskDependencies(milestoneId, token);
    await clearTaskDependencies(childId, token);
    await clearTaskSubtasks(childId, token);
    await setParentTask(childId, milestoneId, token);
    await createDecision(projectId, decisionName, token);

    const progress = await getSubtaskProgress(milestoneId, token);
    expect(progress).toEqual([
      {
        completedSubtasks: 0,
        hasUnavailableSubtasks: false,
        taskId: milestoneId,
        totalSubtasks: 1,
      },
    ]);

    const overviewResponse = page.waitForResponse(isOverviewGet(projectId));
    await gotoApp(page, `/project/${projectId}`);
    expect((await overviewResponse).status()).toBe(200);

    const viewControl = page.getByRole('radiogroup', {
      name: 'Project view',
    });
    await expect(
      viewControl.getByRole('radio', { name: 'List' })
    ).toHaveAttribute('data-checked', '');
    await expectEntityInCurrentList(page, milestoneId, milestoneName);
    await expectEntityInCurrentList(page, childId, childName);

    await page.getByRole('button', { name: 'Show Side Panel' }).click();
    const overviewPanel = page.getByRole('region', { name: 'Overview' });
    await expect(
      overviewPanel.getByText(objective, { exact: true })
    ).toBeVisible();
    await expect(overviewPanel).toContainText('0 of 2 complete');

    await selectProjectView(viewControl, 'Board');
    const board = page.getByRole('region', {
      name: 'Project task status board',
    });
    await expect(board).toBeVisible();
    const childStatus = board.getByLabel(`${childName} status`);
    await expect(childStatus).toHaveValue(NOT_STARTED_STATUS_ID);
    const statusResponse = page.waitForResponse(
      (response) =>
        response.request().method() === 'PUT' &&
        new URL(response.url()).pathname.endsWith(
          `/dss/properties/entities/DOCUMENT/${childId}/${STATUS_PROPERTY_ID}`
        )
    );
    const refreshedOverview = page.waitForResponse(isOverviewGet(projectId));
    await childStatus.selectOption(COMPLETED_STATUS_ID);
    expect((await statusResponse).status()).toBe(204);
    expect((await refreshedOverview).status()).toBe(200);
    await expect(childStatus).toHaveValue(COMPLETED_STATUS_ID);
    await expect(
      page
        .getByRole('region', { name: 'Completed tasks' })
        .getByText(childName, { exact: true })
    ).toBeVisible();
    await expect(board.getByLabel('1 of 1 subtasks complete')).toBeVisible();

    await selectProjectView(viewControl, 'Timeline');
    const timeline = page.getByRole('region', {
      name: 'Project task deadline timeline',
    });
    await expect(timeline).toBeVisible();
    await expect(
      timeline.getByText(milestoneName, { exact: true })
    ).toBeVisible();
    await expect(timeline.getByLabel('1 of 1 subtasks complete')).toBeVisible();
    await expect(timeline.getByText(childName, { exact: true })).toBeVisible();

    await selectProjectView(viewControl, 'Decisions');
    await expect(page.getByText(decisionName, { exact: true })).toBeVisible();

    await selectProjectView(viewControl, 'Reports');
    await expect(
      page.getByRole('heading', { name: 'Project report' })
    ).toBeVisible();
    const currentHealth = page.getByRole('region', { name: 'Current health' });
    await expect(metric(currentHealth, 'Completion rate')).toContainText('50%');
    await expect(metric(currentHealth, 'Work in progress')).toContainText('0');
    await expect(metric(currentHealth, 'Milestones at risk')).toContainText(
      '0'
    );
    await expect(
      page.getByText(
        'Throughput and lead time are unavailable until complete task transition history is recorded.',
        { exact: true }
      )
    ).toBeVisible();

    await expect(overviewPanel).toContainText('1 of 2 complete');
    await selectProjectView(viewControl, 'List');
    const milestoneRow = page.locator(entityIdSelector(milestoneId)).first();
    const childRow = page.locator(entityIdSelector(childId)).first();
    await expect(milestoneRow).toContainText(milestoneName);
    await expect(childRow).toContainText(childName);
    await expect(childRow).toContainText('Completed');
  } finally {
    if (projectId) await deleteProject(projectId, token);
  }
});

type PropertyInput = {
  propertyId: string;
  value:
    | { type: 'boolean'; value: boolean }
    | { type: 'date'; value: string }
    | { type: 'select_option'; option_id: string };
};

type ProjectOperations = {
  updatedAt: string;
};

type Progress = {
  completedSubtasks: number;
  hasUnavailableSubtasks: boolean;
  taskId: string;
  totalSubtasks: number;
};

function backendOrigin(): string {
  const origin = process.env.LOCAL_E2E_BACKEND_ORIGIN;
  if (!origin) throw new Error('LOCAL_E2E_BACKEND_ORIGIN is required');
  return origin;
}

function headers(token: string) {
  return {
    Authorization: `Bearer ${token}`,
    'Content-Type': 'application/json',
  };
}

async function replaceOperations(
  projectId: string,
  token: string,
  fields: { objective: string; startDate: string; targetDate: string }
) {
  const url = `${backendOrigin()}/dss/v2/projects/${projectId}/operations`;
  const current = await fetch(url, { headers: headers(token) });
  expect(current.status).toBe(200);
  const currentBody = (await current.json()) as { data?: ProjectOperations };
  if (!currentBody.data)
    throw new Error('Project operations response did not contain data');

  const response = await fetch(url, {
    method: 'PUT',
    headers: headers(token),
    body: JSON.stringify({
      expectedUpdatedAt: currentBody.data.updatedAt,
      leadUserId: null,
      nextAction: 'Review the canonical manager views',
      objective: fields.objective,
      policy: null,
      priority: 'high',
      startDate: fields.startDate,
      status: 'active',
      targetDate: fields.targetDate,
    }),
  });
  expect(response.status).toBe(200);
}

async function createTask(
  projectId: string,
  taskName: string,
  token: string,
  propertyValues: PropertyInput[]
): Promise<string> {
  const response = await fetch(`${backendOrigin()}/dss/documents/create_task`, {
    method: 'POST',
    headers: headers(token),
    body: JSON.stringify({
      markdown: '',
      projectId,
      propertyValues,
      taskName,
    }),
  });
  expect(response.status).toBe(200);
  return documentIdFrom(await response.json(), 'Task');
}

async function setParentTask(childId: string, parentId: string, token: string) {
  const response = await fetch(
    `${backendOrigin()}/dss/properties/entities/DOCUMENT/${childId}/${PARENT_TASK_PROPERTY_ID}`,
    {
      method: 'PUT',
      headers: headers(token),
      body: JSON.stringify({
        value: {
          type: 'entity_reference',
          reference: { entity_id: parentId, entity_type: 'TASK' },
        },
      }),
    }
  );
  expect(response.status).toBe(204);
}

async function clearTaskDependencies(taskId: string, token: string) {
  await setEmptyTaskRelation(taskId, DEPENDS_ON_PROPERTY_ID, token);
}

async function clearTaskSubtasks(taskId: string, token: string) {
  await setEmptyTaskRelation(taskId, SUBTASKS_PROPERTY_ID, token);
}

async function setEmptyTaskRelation(
  taskId: string,
  propertyId: string,
  token: string
) {
  const response = await fetch(
    `${backendOrigin()}/dss/properties/entities/DOCUMENT/${taskId}/${propertyId}`,
    {
      method: 'PUT',
      headers: headers(token),
      body: JSON.stringify({
        value: { type: 'multi_entity_reference', references: [] },
      }),
    }
  );
  expect(response.status).toBe(204);
}

async function createDecision(
  projectId: string,
  decisionName: string,
  token: string
) {
  const response = await fetch(
    `${backendOrigin()}/dss/documents/create_decision`,
    {
      method: 'POST',
      headers: headers(token),
      body: JSON.stringify({ decisionName, markdown: '', projectId }),
    }
  );
  expect(response.status).toBe(200);
  documentIdFrom(await response.json(), 'Decision');
}

async function getSubtaskProgress(
  taskId: string,
  token: string
): Promise<Progress[]> {
  const response = await fetch(
    `${backendOrigin()}/dss/properties/task-subtask-progress`,
    {
      method: 'POST',
      headers: headers(token),
      body: JSON.stringify({ taskIds: [taskId] }),
    }
  );
  expect(response.status).toBe(200);
  return (await response.json()) as Progress[];
}

function metric(region: Locator, label: string) {
  return region.locator('dt', { hasText: label }).locator('..');
}

async function selectProjectView(viewControl: Locator, name: string) {
  const radio = viewControl.getByRole('radio', { name });
  await radio.locator('..').click();
  await expect(radio).toBeChecked();
}

function selectProperty(propertyId: string, optionId: string): PropertyInput {
  return {
    propertyId,
    value: { type: 'select_option', option_id: optionId },
  };
}

function dateProperty(
  propertyId: string,
  daysFromToday: number
): PropertyInput {
  return {
    propertyId,
    value: { type: 'date', value: `${dateOnly(daysFromToday)}T12:00:00.000Z` },
  };
}

function dateOnly(daysFromToday: number): string {
  const date = new Date();
  date.setUTCDate(date.getUTCDate() + daysFromToday);
  return date.toISOString().slice(0, 10);
}

function projectIdFrom(response: unknown): string {
  const id = (response as { data?: { id?: unknown } }).data?.id;
  if (typeof id !== 'string')
    throw new Error('Project creation did not return an id');
  return id;
}

function documentIdFrom(response: unknown, kind: string): string {
  const body = response as {
    data?: { documentId?: unknown };
    documentId?: unknown;
  };
  const id = body.data?.documentId ?? body.documentId;
  if (typeof id !== 'string')
    throw new Error(`${kind} creation did not return a document id`);
  return id;
}

function isOverviewGet(projectId: string) {
  return (response: import('@playwright/test').Response) =>
    response.request().method() === 'GET' &&
    new URL(response.url()).pathname.endsWith(
      `/dss/v2/projects/${projectId}/overview`
    );
}

async function deleteProject(projectId: string, token: string) {
  const requestHeaders = { Authorization: `Bearer ${token}` };
  const softDelete = await fetch(
    `${backendOrigin()}/dss/projects/${projectId}`,
    { method: 'DELETE', headers: requestHeaders }
  );
  if (softDelete.status !== 200 && softDelete.status !== 404) {
    throw new Error(`Project soft delete failed with ${softDelete.status}`);
  }
  const permanentDelete = await fetch(
    `${backendOrigin()}/dss/projects/${projectId}/permanent`,
    { method: 'DELETE', headers: requestHeaders }
  );
  if (permanentDelete.status !== 200 && permanentDelete.status !== 404) {
    throw new Error(
      `Project permanent delete failed with ${permanentDelete.status}`
    );
  }
}

function tokenFor(email: string): string {
  return execFileSync(
    'bun',
    ['scripts/generate-local-e2e-token.ts', `--email=${email}`],
    {
      cwd: process.cwd(),
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }
  ).trim();
}
