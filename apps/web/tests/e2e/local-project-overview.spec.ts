import { execFileSync } from 'node:child_process';

import { expect, test } from '@playwright/test';

import { gotoApp, LOCAL_E2E, uniqueE2EText } from './helpers/local-app';

test.skip(!LOCAL_E2E, 'requires LOCAL_E2E=true and the isolated smoke seed');

test('edits canonical project narratives and immediately reuses the refreshed version', async ({
  page,
}) => {
  test.setTimeout(300_000);
  const token = tokenFor('manager@inimoa.local');
  const objective = uniqueE2EText('WS-03 objective');
  const firstAction = uniqueE2EText('WS-03 first action');
  const secondAction = uniqueE2EText('WS-03 second action');
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
    const initial = await getOperations(projectId, token);
    expect(initial.objective).toBeNull();
    expect(initial.nextAction).toBeNull();

    const showSidePanel = page.getByRole('button', {
      name: 'Show Side Panel',
    });
    await expect(showSidePanel).toBeVisible();
    await showSidePanel.click();

    const editProjectDetails = page.getByRole('button', {
      name: 'Edit project details',
    });
    await expect(editProjectDetails).toBeVisible();
    await editProjectDetails.click();
    let dialog = page.getByRole('dialog');
    await dialog.getByLabel('Objective').fill(`  ${objective}  `);
    await dialog.getByLabel('Next action').fill(`  ${firstAction}  `);

    const firstPut = page.waitForResponse(isOperationsPut(projectId));
    const firstOverview = page.waitForResponse(isOverviewGet(projectId));
    await dialog.getByRole('button', { name: 'Save' }).click();
    expect((await firstPut).status()).toBe(200);
    expect((await firstOverview).status()).toBe(200);
    await expect(dialog).not.toBeVisible();
    await expect(page.getByText(objective, { exact: true })).toBeVisible();
    await expect(page.getByText(firstAction, { exact: true })).toBeVisible();

    await editProjectDetails.click();
    dialog = page.getByRole('dialog');
    await expect(dialog.getByLabel('Objective')).toHaveValue(objective);
    await expect(dialog.getByLabel('Next action')).toHaveValue(firstAction);
    await dialog.getByLabel('Next action').fill(secondAction);

    const secondPut = page.waitForResponse(isOperationsPut(projectId));
    const secondOverview = page.waitForResponse(isOverviewGet(projectId));
    await dialog.getByRole('button', { name: 'Save' }).click();
    expect((await secondPut).status()).toBe(200);
    expect((await secondOverview).status()).toBe(200);
    await expect(page.getByText(secondAction, { exact: true })).toBeVisible();
    const canonical = await getOperations(projectId, token);
    expect(canonical.objective).toBe(objective);
    expect(canonical.nextAction).toBe(secondAction);
  } finally {
    if (projectId) await deleteProject(projectId, token);
  }
});

type Operations = {
  objective?: string | null;
  nextAction?: string | null;
};

function backendOrigin(): string {
  const origin = process.env.LOCAL_E2E_BACKEND_ORIGIN;
  if (!origin) throw new Error('LOCAL_E2E_BACKEND_ORIGIN is required');
  return origin;
}

function operationsUrl(projectId: string): string {
  return `${backendOrigin()}/dss/v2/projects/${projectId}/operations`;
}

async function getOperations(
  projectId: string,
  token: string
): Promise<Operations> {
  const response = await fetch(operationsUrl(projectId), {
    headers: { Authorization: `Bearer ${token}` },
  });
  expect(response.status).toBe(200);
  const body = (await response.json()) as { data?: Operations };
  if (!body.data) throw new Error('Operations response did not contain data');
  return body.data;
}

function isOperationsPut(projectId: string) {
  return (response: import('@playwright/test').Response) =>
    response.request().method() === 'PUT' &&
    new URL(response.url()).pathname.endsWith(
      `/dss/v2/projects/${projectId}/operations`
    );
}

function isOverviewGet(projectId: string) {
  return (response: import('@playwright/test').Response) =>
    response.request().method() === 'GET' &&
    new URL(response.url()).pathname.endsWith(
      `/dss/v2/projects/${projectId}/overview`
    );
}

function projectIdFrom(response: unknown): string {
  const id = (response as { data?: { id?: unknown } }).data?.id;
  if (typeof id !== 'string')
    throw new Error('Project creation did not return an id');
  return id;
}

async function deleteProject(projectId: string, token: string) {
  const headers = { Authorization: `Bearer ${token}` };
  const softDelete = await fetch(
    `${backendOrigin()}/dss/projects/${projectId}`,
    { method: 'DELETE', headers }
  );
  if (softDelete.status !== 200 && softDelete.status !== 404) {
    throw new Error(`Project soft delete failed with ${softDelete.status}`);
  }
  const permanentDelete = await fetch(
    `${backendOrigin()}/dss/projects/${projectId}/permanent`,
    { method: 'DELETE', headers }
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
