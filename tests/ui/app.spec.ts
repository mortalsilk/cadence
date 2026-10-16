import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    let active: any = null;
    let active_session: any = null;
    let records: any[] = [];
    let sessions: any[] = [];
    let window_mode: 'normal' | 'mini' = 'normal';
    let sequence = 0;
    const callbacks = new Map<number, (value: any) => void>();
    const date = () => {
      const value = new Date();
      return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, '0')}-${String(value.getDate()).padStart(2, '0')}`;
    };
    const view = () => ({ active, active_session, window_mode });
    (window as any).__TAURI_INTERNALS__ = {
      transformCallback(callback: (value: any) => void) {
        const id = ++sequence;
        callbacks.set(id, callback);
        return id;
      },
      unregisterCallback(id: number) {
        callbacks.delete(id);
      },
      async invoke(command: string, args: any = {}) {
        if (command === 'plugin:event|listen') return ++sequence;
        if (command === 'plugin:event|unlisten') return;
        if (command === 'get_state') return view();
        if (command === 'set_presentation_mode') {
          window_mode = args.mode;
          return window_mode;
        }
        if (command === 'start_session') {
          active_session = {
            id: `session-${++sequence}`,
            ...args.input,
            elapsed_ms: 0,
            phase: 'running',
            started_at: new Date().toISOString(),
          };
          return view();
        }
        if (command === 'session_action') {
          if (args.action === 'discard') active_session = null;
          else
            active_session = {
              ...active_session,
              phase:
                args.action === 'pause'
                  ? 'paused'
                  : args.action === 'resume'
                    ? 'running'
                    : 'review',
              elapsed_ms: 3_600_000,
              date: date(),
              finished_at: new Date().toISOString(),
            };
          return view();
        }
        if (command === 'save_session') {
          sessions.push({ ...active_session, ...args.input });
          const saved = sessions.at(-1);
          active_session = null;
          return saved;
        }
        if (command === 'start_stint') {
          active = {
            id: `stint-${++sequence}`,
            label: args.label,
            mode: args.mode,
            elapsed_ms: 0,
            target_ms: args.mode === 'countdown' ? args.minutes * 60_000 : null,
            phase: 'running',
            started_at: new Date().toISOString(),
            session_id: active_session?.id ?? null,
          };
          return view();
        }
        if (command === 'timer_action') {
          if (args.action === 'discard') active = null;
          else
            active = {
              ...active,
              phase:
                args.action === 'pause'
                  ? 'paused'
                  : args.action === 'resume'
                    ? 'running'
                    : 'review',
              elapsed_ms: 65_000,
              date: date(),
            };
          return view();
        }
        if (command === 'save_stint') {
          const stint = {
            ...active,
            label: args.label,
            description: args.description,
            date: date(),
            finished_at: new Date().toISOString(),
          };
          records.push(stint);
          active = null;
          return stint;
        }
        if (command === 'list_stints') return records;
        if (command === 'list_sessions') return sessions;
        if (command === 'daily_summaries') {
          const days: any = {};
          for (const session of sessions) {
            days[session.date] ??= {
              count: 0,
              session_count: 0,
              elapsed_ms: 0,
            };
            days[session.date].session_count++;
          }
          for (const stint of records) {
            const session = sessions.find(
              (item) => item.id === stint.session_id,
            );
            const day = session?.date ?? stint.date;
            days[day] ??= { count: 0, session_count: 0, elapsed_ms: 0 };
            days[day].count++;
            days[day].elapsed_ms += stint.elapsed_ms;
          }
          return days;
        }
        if (command === 'edit_stint') {
          const old = records.find((item) => item.id === args.id);
          const stint = {
            ...(old ?? {
              id: `manual-${++sequence}`,
              mode: 'manual',
              finished_at: new Date().toISOString(),
              target_ms: null,
            }),
            ...args.input,
          };
          records = records.filter((item) => item.id !== args.id);
          records.push(stint);
          return stint;
        }
        if (command === 'delete_stint') {
          records = records.filter((item) => item.id !== args.id);
          return;
        }
        if (command === 'edit_session') {
          sessions = sessions.map((item) =>
            item.id === args.id ? { ...item, ...args.input } : item,
          );
          return sessions.find((item) => item.id === args.id);
        }
        if (command === 'delete_session') {
          records = records.map((item) =>
            item.session_id === args.id ? { ...item, session_id: null } : item,
          );
          sessions = sessions.filter((item) => item.id !== args.id);
          return;
        }
        throw new Error(`Unhandled test IPC: ${command}`);
      },
    };
  });
  await page.goto('/');
});

async function saveCurrentStint(
  page: import('@playwright/test').Page,
  name: string,
) {
  await page.getByRole('button', { name: 'Start stint' }).click();
  await page.getByRole('button', { name: 'End stint' }).click();
  await page.getByLabel('Stint name').fill(name);
  await page.getByRole('button', { name: 'Save stint' }).click();
}

test('records, edits, and deletes a standalone stint', async ({ page }) => {
  await page.getByLabel('Stint label').fill('Programming');
  await page.getByRole('button', { name: 'Start stint' }).click();
  await expect(page.getByText('RUNNING', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: /Pause/ }).click();
  await expect(page.getByText('PAUSED', { exact: true }).first()).toBeVisible();
  await page.getByRole('button', { name: /Resume/ }).click();
  await page.getByRole('button', { name: 'End stint' }).click();
  await expect(page.getByText('Stint ended.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Save stint' }).click();
  await page.getByRole('button', { name: '02 History' }).click();
  await page.getByRole('button', { name: 'Edit Programming' }).click();
  await page.getByLabel('Name', { exact: true }).fill('Deep work');
  await page.getByRole('button', { name: 'Save stint' }).click();
  await page.getByRole('button', { name: 'Delete Deep work' }).click();
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.getByText('No records.')).toBeVisible();
});

test('records a multi-stint session and preserves stints when deleting it', async ({
  page,
}) => {
  await page.getByRole('button', { name: '+ Start session' }).click();
  await page.getByLabel('Session name').fill('Morning block');
  await page
    .getByRole('dialog')
    .getByRole('button', { name: /Start session/ })
    .click();
  await expect(
    page.getByRole('heading', { name: 'Morning block' }),
  ).toBeVisible();
  await page.getByLabel('Stint label').fill('Research');
  await saveCurrentStint(page, 'Research');
  await page.getByLabel('Stint label').fill('Writing');
  await saveCurrentStint(page, 'Writing');
  await page.getByRole('button', { name: 'End session' }).click();
  await expect(page.getByText('Session ended', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Save session' }).click();
  await page.getByRole('button', { name: '02 History' }).click();
  await expect(
    page.getByRole('button', { name: 'Edit session Morning block' }),
  ).toBeVisible();
  await expect(page.locator('.session-card .nested-stints li')).toHaveCount(2);
  await page
    .getByRole('button', { name: 'Delete session Morning block' })
    .click();
  await expect(
    page.getByText('Its stints will remain as standalone records.'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.getByText('STANDALONE STINTS')).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Edit Research' }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Edit Writing' }),
  ).toBeVisible();
});

test('supports manual session assignment and neutral visual language', async ({
  page,
}) => {
  await page.getByRole('button', { name: '+ Start session' }).click();
  await page.getByLabel('Session name').fill('Recorded session');
  await page
    .getByRole('dialog')
    .getByRole('button', { name: /Start session/ })
    .click();
  await page.getByRole('button', { name: 'End session' }).click();
  await page.getByRole('button', { name: 'Save session' }).click();
  await page.getByRole('button', { name: '02 History' }).click();
  await page.getByRole('button', { name: 'Add stint' }).click();
  await page.getByLabel('Name', { exact: true }).fill('Manual entry');
  await page.getByLabel('Minutes').fill('45');
  await page.getByLabel('Session', { exact: true }).selectOption({ index: 1 });
  await page.getByRole('button', { name: 'Save stint' }).click();
  await expect(page.locator('.session-card .nested-stints li')).toHaveCount(1);

  const body = await page.locator('body').innerText();
  for (const removed of [
    'YOUR TIME. YOUR PACE.',
    'Time well spent.',
    'Find your rhythm.',
    'A little space for progress.',
    'KEEP YOUR PACE.',
  ])
    expect(body).not.toContain(removed);
  expect(
    await page
      .locator('.brand')
      .evaluate((element) => getComputedStyle(element).fontStyle),
  ).toBe('italic');
  expect(
    await page.evaluate(() =>
      getComputedStyle(document.documentElement)
        .getPropertyValue('--red')
        .trim(),
    ),
  ).toBe('#e10600');
});

test('fits supported layouts, keyboard dialogs, and reduced motion', async ({
  page,
}) => {
  await expect(page.locator('.day')).toHaveCount(0);
  await page.getByRole('button', { name: '02 History' }).click();
  await expect(page.locator('.day')).toHaveCount(84);
  await page.getByRole('button', { name: 'Export and backup' }).click();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).not.toBeVisible();
  for (const viewport of [
    { width: 360, height: 540 },
    { width: 380, height: 620 },
    { width: 570, height: 930 },
  ]) {
    await page.setViewportSize(viewport);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await page.setViewportSize({ width: 570, height: 930 });
  await page.evaluate(() => (document.documentElement.style.zoom = '1.5'));
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  expect(
    await page
      .locator('.add-button')
      .evaluate((element) => getComputedStyle(element).transitionDuration),
  ).toBe('0s');
});

test('switches to a compact timer capsule and restores the normal view', async ({
  page,
}) => {
  await page.getByLabel('Stint label').fill('Capsule task');
  await page.getByRole('button', { name: 'Start stint' }).click();
  await page.getByRole('button', { name: 'Mini mode' }).click();
  await expect(page.getByLabel('cadence mini timer')).toBeVisible();
  await expect(page.getByRole('timer')).toBeVisible();
  await page.getByRole('button', { name: 'Pause stint' }).click();
  await expect(page.getByRole('button', { name: 'Resume stint' })).toBeVisible();
  await page.getByRole('button', { name: 'End stint' }).click();
  await expect(page.getByText('Stint ended.', { exact: true })).toBeVisible();
});
