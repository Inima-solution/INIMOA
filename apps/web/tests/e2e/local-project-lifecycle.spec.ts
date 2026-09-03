import { execFileSync } from 'node:child_process';

import { expect, test } from '@playwright/test';

import { localE2ESeed } from './fixtures/local-e2e-seed';
import { gotoApp, LOCAL_E2E, uniqueE2EText } from './helpers/local-app';

test.skip(!LOCAL_E2E, 'requires LOCAL_E2E=true and the isolated smoke seed');

test.describe
  .serial('local project lifecycle', () => {
    // A cold Linux Vite transform can consume most of the default minute
    // before the first project screen becomes interactive.
    test.describe.configure({ timeout: 300_000 });

    test('creates, moves content, verifies access, restores, and permanently deletes one project', async ({
      page,
    }) => {
      const renamedName = uniqueE2EText('local lifecycle renamed');
      const moveTargetName = uniqueE2EText('local lifecycle move target');
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
        const newFolderTitle = page
          .getByText('New Folder', { exact: true })
          .first();
        await expect(newFolderTitle).toBeVisible();
        await page.locator('[data-split-container]').first().click();
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

        const moveTargetResponsePromise = page.waitForResponse(
          (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname.endsWith('/dss/projects')
        );
        await page.getByRole('button', { name: 'Create', exact: true }).click();
        await page
          .getByRole('menuitem', { name: 'Folder', exact: true })
          .click();
        const moveTargetResponse = await moveTargetResponsePromise;
        expect(moveTargetResponse.status()).toBe(200);
        const moveTargetId = projectIdFrom(await moveTargetResponse.json());
        await expect(page).toHaveURL(new RegExp(moveTargetId));
        await page.locator('[data-split-container]').first().click();
        await page.keyboard.press('r');
        const moveTargetRenameDialog = page
          .getByRole('dialog')
          .filter({ hasText: 'Rename' });
        const moveTargetRenameInput =
          moveTargetRenameDialog.getByPlaceholder('Enter new text...');
        await expect(moveTargetRenameInput).toBeVisible();
        await moveTargetRenameInput.fill(moveTargetName);
        await moveTargetRenameDialog
          .getByRole('button', { name: 'Rename', exact: true })
          .click();
        await expect(
          page.getByText(moveTargetName, { exact: true }).first()
        ).toBeVisible();

        await gotoApp(page, `/project/${id}`);
        const noteResponsePromise = page.waitForResponse(
          (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname.endsWith(
              '/dss/documents/create_markdown'
            )
        );
        await page.getByRole('button', { name: 'Create', exact: true }).click();
        await page.getByRole('menuitem', { name: 'Note', exact: true }).click();
        const noteResponse = await noteResponsePromise;
        expect(noteResponse.status()).toBe(200);
        const noteId = documentIdFrom(await noteResponse.json());
        await expect(page).toHaveURL(new RegExp(noteId));

        const noteTitle = page.getByText('New Note', { exact: true }).first();
        await expect(noteTitle).toBeVisible();
        await noteTitle.click({ button: 'right' });
        await page.getByRole('menuitem', { name: /^Move to Folder/ }).click();
        const folderSearch = page.getByPlaceholder('Search folders...');
        await expect(folderSearch).toBeVisible();
        await folderSearch.fill(moveTargetName);
        await page.getByText(moveTargetName, { exact: true }).first().click();
        const moveResponsePromise = page.waitForResponse(
          (response) =>
            response.request().method() === 'PATCH' &&
            new URL(response.url()).pathname.endsWith(
              `/dss/documents/${noteId}`
            )
        );
        await page
          .getByRole('button', { name: 'Move', exact: true })
          .last()
          .click();
        expect((await moveResponsePromise).status()).toBe(200);
        await expect(
          page.getByText('Moved to folder', { exact: true })
        ).toBeVisible();
        await gotoApp(page, `/project/${moveTargetId}`);
        await expect(
          page
            .locator('[data-split-container]')
            .first()
            .getByText('New Note', { exact: true })
            .first()
        ).toBeVisible();

        await gotoApp(page, `/project/${id}`);
        const shareButton = page
          .getByRole('button', { name: 'Share', exact: true })
          .first();
        await expect(shareButton).toBeVisible();
        await shareButton.click();
        const shareDialog = page
          .getByRole('dialog')
          .filter({ hasText: `Share:${renamedName}` });
        await expect(shareDialog).toBeVisible();
        await shareDialog
          .getByRole('button', { name: 'Cancel', exact: true })
          .click();
        await expect(shareDialog).not.toBeVisible();

        const chatResponsePromise = page.waitForResponse(
          (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname.endsWith('/chats')
        );
        await page.getByText('Chat', { exact: true }).first().click();
        const chatResponse = await chatResponsePromise;
        expect(chatResponse.status()).toBe(200);
        const chatId = chatIdFrom(await chatResponse.json());
        await expect(page).toHaveURL(new RegExp(chatId));

        await gotoApp(page, `/project/${id}`);
        const uploadName = `${uniqueE2EText('local-lifecycle-upload').replaceAll(' ', '-')}.txt`;
        const uploadCreatedPromise = page.waitForResponse(
          (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname.endsWith('/dss/documents')
        );
        const dataTransfer = await page.evaluateHandle((name) => {
          const transfer = new DataTransfer();
          transfer.items.add(
            new File(['local project lifecycle upload'], name, {
              type: 'text/plain',
            })
          );
          return transfer;
        }, uploadName);
        const dropTarget = page
          .locator('[data-split-container]')
          .first()
          .locator('div.size-full.bg-surface.flex.flex-col.relative')
          .first();
        await dropTarget.dispatchEvent('dragenter', { dataTransfer });
        await expect(
          page.getByText('Upload to this folder', { exact: true })
        ).toBeVisible();
        await dropTarget.dispatchEvent('drop', { dataTransfer });
        expect((await uploadCreatedPromise).status()).toBe(200);
        await expect(
          page.getByText(`Uploaded ${uploadName.slice(0, -4)}`, {
            exact: true,
          })
        ).toBeVisible({ timeout: 60_000 });

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

function documentIdFrom(response: unknown): string {
  const body = response as {
    data?: { documentId?: unknown };
    documentId?: unknown;
  };
  const id = body.data?.documentId ?? body.documentId;
  if (typeof id !== 'string')
    throw new Error('Document creation did not return an id');
  return id;
}

function chatIdFrom(response: unknown): string {
  const body = response as { data?: { id?: unknown }; id?: unknown };
  const id = body.data?.id ?? body.id;
  if (typeof id !== 'string')
    throw new Error('Chat creation did not return an id');
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
