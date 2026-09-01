import { execFileSync } from 'node:child_process';

import { expect, test } from '@playwright/test';

import { localE2ESeed } from './fixtures/local-e2e-seed';
import { gotoApp, LOCAL_E2E, uniqueE2EText } from './helpers/local-app';

test.skip(!LOCAL_E2E, 'requires LOCAL_E2E=true and the isolated smoke seed');

test.describe
  .serial('local project lifecycle', () => {
    test.describe.configure({ timeout: 60_000 });

    test('creates, renames, verifies access, restores, and permanently deletes one project', async ({
      page,
    }) => {
      const renamedName = uniqueE2EText('local lifecycle renamed');
      let projectId: string | undefined;
      let cleanupComplete = false;
      const ownerToken = tokenFor();
      const otherUser = localE2ESeed.users.find(
        (user) => user.email !== localE2ESeed.smoke.user.email
      );
      expect(otherUser).toBeDefined();
      const otherToken = tokenFor(otherUser!.email);

      try {
        await gotoApp(page, '/component/documents');

        const createResponse = page.waitForResponse(
          (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname.endsWith('/dss/projects')
        );
        await page.keyboard.press('c');
        await expect(
          page.getByText('Create New', { exact: true })
        ).toBeVisible();
        await page.keyboard.press('f');
        const created = await createResponse;
        expect(created.status()).toBe(200);
        const id = projectIdFrom(await created.json());
        projectId = id;
        expect(id).toMatch(
          /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i
        );

        await expect(page).toHaveURL(new RegExp(id));
        await expect(
          page.getByText('New Folder', { exact: true }).first()
        ).toBeVisible();
        await page.keyboard.press('r');
        const renameDialog = page
          .getByRole('dialog')
          .filter({ hasText: 'Rename' });
        const renameInput = renameDialog.getByPlaceholder('Enter new text...');
        await expect(renameInput).toBeVisible();
        await renameInput.fill(renamedName);
        await renameDialog
          .getByRole('button', { name: 'Rename', exact: true })
          .click();
        await expect(
          page.getByText(renamedName, { exact: true }).first()
        ).toBeVisible();

        const showSidePanel = page.getByRole('button', {
          name: 'Show Side Panel',
        });
        if (await showSidePanel.isVisible()) await showSidePanel.click();
        else
          await expect(
            page.getByRole('button', { name: 'Hide Side Panel' })
          ).toBeVisible();
        await expect(
          page.getByText('Project overview is not available to you.', {
            exact: true,
          })
        ).toBeVisible();

        const unauthorized = await requestProject(id, otherToken, 'GET');
        expect(unauthorized.status).toBe(401);

        await softDeleteThroughUi(page, id, renamedName);

        expect(
          (await requestProject(id, ownerToken, 'PUT', '/revert_delete')).status
        ).toBe(200);
        expect((await requestProject(id, ownerToken, 'GET')).status).toBe(200);
        await gotoApp(page, `/project/${id}`);
        await expect(
          page.getByText(renamedName, { exact: true }).first()
        ).toBeVisible();

        const restoredShowSidePanel = page.getByRole('button', {
          name: 'Show Side Panel',
        });
        if (await restoredShowSidePanel.isVisible())
          await restoredShowSidePanel.click();
        await expect(
          page.getByText('Project overview is not available to you.', {
            exact: true,
          })
        ).toBeVisible();

        await softDeleteThroughUi(page, id, renamedName);
        expect(
          (await requestProject(id, ownerToken, 'DELETE', '/permanent')).status
        ).toBe(200);
        expect((await requestProject(id, ownerToken, 'GET')).status).toBe(404);
        cleanupComplete = true;
      } finally {
        if (projectId && !cleanupComplete) {
          await requestProject(projectId, ownerToken, 'DELETE').catch(
            () => undefined
          );
          await requestProject(
            projectId,
            ownerToken,
            'DELETE',
            '/permanent'
          ).catch(() => undefined);
        }
      }
    });
  });

function projectIdFrom(response: unknown): string {
  const id = (response as { data?: { id?: unknown } }).data?.id;
  if (typeof id !== 'string')
    throw new Error('Project creation did not return an id');
  return id;
}

function tokenFor(email?: string): string {
  return execFileSync(
    'bun',
    [
      'scripts/generate-local-e2e-token.ts',
      ...(email ? [`--email=${email}`] : []),
    ],
    {
      cwd: process.cwd(),
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }
  ).trim();
}

async function requestProject(
  id: string,
  token: string,
  method: 'GET' | 'PUT' | 'DELETE' = 'GET',
  suffix?: string
) {
  const origin = process.env.LOCAL_E2E_BACKEND_ORIGIN;
  if (!origin) throw new Error('LOCAL_E2E_BACKEND_ORIGIN is required');
  const path = suffix ?? (method === 'PUT' ? '/revert_delete' : '');
  const response = await fetch(`${origin}/dss/projects/${id}${path}`, {
    method,
    headers: { Authorization: `Bearer ${token}` },
  });
  return { status: response.status };
}

async function softDeleteThroughUi(
  page: import('@playwright/test').Page,
  id: string,
  name: string
) {
  const title = page.getByText(name, { exact: true }).first();
  await expect(title).toBeVisible();
  await title.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Delete', exact: true }).click();
  const deleteResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'DELETE' &&
      new URL(response.url()).pathname.endsWith(`/dss/projects/${id}`)
  );
  await page
    .getByRole('button', { name: 'Delete', exact: true })
    .last()
    .click();
  expect((await deleteResponse).status()).toBe(200);
}
