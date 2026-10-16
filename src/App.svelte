<script lang="ts">
  import { onMount } from 'svelte';
  import { api, desktop } from './api';
  import {
    calendarDays,
    clock,
    duration,
    friendly,
    localDate,
  } from './calendar';
  import type {
    Active,
    ActiveSession,
    DaySummary,
    ImportPreview,
    Session,
    Stint,
    WindowMode,
  } from './types';

  type Modal =
    | 'entry'
    | 'deleteStint'
    | 'discardStint'
    | 'data'
    | 'import'
    | 'startSession'
    | 'reviewSession'
    | 'editSession'
    | 'deleteSession'
    | 'discardSession';

  let tab = $state<'timer' | 'history'>('timer');
  let active = $state<Active | null>(null);
  let activeSession = $state<ActiveSession | null>(null);
  let records = $state<Stint[]>([]);
  let sessions = $state<Session[]>([]);
  let summaries = $state<Record<string, DaySummary>>({});
  let mode = $state<'count_up' | 'countdown'>('count_up');
  let minutes = $state(25);
  let label = $state('');
  let reviewLabel = $state('');
  let description = $state('');
  let reviewId = '';
  let sessionReviewId = '';
  let lastWarning = '';
  let busy = $state(false);
  let ready = $state(false);
  let error = $state('');
  let notice = $state('');
  let today = $state(localDate());
  let offset = $state(0);
  let selected = $state(localDate());
  let modal = $state<Modal | null>(null);
  let dialog: HTMLDialogElement;
  let editId = $state<string | null>(null);
  let editLabel = $state('');
  let editDescription = $state('');
  let editDate = $state(localDate());
  let editMinutes = $state(25);
  let editSeconds = $state(0);
  let editSessionId = $state('');
  let originalDuration = 0;
  let initialMinutes = 0;
  let initialSeconds = 0;
  let deleteId = $state('');
  let deleteLabel = $state('');
  let sessionId = $state('');
  let sessionLabel = $state('');
  let sessionDescription = $state('');
  let preview = $state<ImportPreview | null>(null);
  let windowMode = $state<WindowMode>('normal');
  const mini = $derived(windowMode === 'mini');

  const days = $derived(calendarDays(offset, new Date(`${today}T12:00:00`)));
  const selectedSessions = $derived(
    sessions.filter((session) => session.date === selected),
  );
  const selectedStandalone = $derived(
    records
      .filter((stint) => !stint.session_id && stint.date === selected)
      .sort((a, b) => b.finished_at.localeCompare(a.finished_at)),
  );
  const selectedSummary = $derived(
    summaries[selected] ?? { count: 0, session_count: 0, elapsed_ms: 0 },
  );
  const todaySummary = $derived(
    summaries[today] ?? { count: 0, session_count: 0, elapsed_ms: 0 },
  );
  const activeSessionStints = $derived(
    activeSession
      ? records.filter((stint) => stint.session_id === activeSession?.id)
      : [],
  );
  const activeSessionTime = $derived(
    activeSessionStints.reduce((sum, stint) => sum + stint.elapsed_ms, 0),
  );
  const running = $derived(active?.phase === 'running');
  const sessionRunning = $derived(activeSession?.phase === 'running');
  const isReview = $derived(active?.phase === 'review');
  const displayTime = $derived(
    active
      ? active.target_ms === null
        ? active.elapsed_ms
        : Math.max(0, active.target_ms - active.elapsed_ms)
      : mode === 'countdown'
        ? (minutes || 0) * 60000
        : 0,
  );
  const progress = $derived(
    active?.target_ms
      ? Math.min(100, (active.elapsed_ms / active.target_ms) * 100)
      : 0,
  );

  $effect(() => {
    if (active?.phase === 'review' && reviewId !== active.id) {
      reviewId = active.id;
      reviewLabel = active.label;
      description = '';
      tab = 'timer';
    }
    if (!active) reviewId = '';
  });
  $effect(() => {
    if (
      !mini &&
      activeSession?.phase === 'review' &&
      sessionReviewId !== activeSession.id
    ) {
      sessionReviewId = activeSession.id;
      sessionLabel = activeSession.label;
      sessionDescription = activeSession.description;
      modal = 'reviewSession';
    }
    if (!activeSession) sessionReviewId = '';
  });
  $effect(() => {
    if (dialog) {
      if (modal && !dialog.open) dialog.showModal();
      else if (!modal && dialog.open) dialog.close();
    }
  });

  async function refresh() {
    [records, sessions, summaries] = await Promise.all([
      api.list(),
      api.listSessions(),
      api.summaries(),
    ]);
  }
  function applyView(view: {
    active: Active | null;
    active_session: ActiveSession | null;
    window_mode?: WindowMode;
  }) {
    active = view.active;
    activeSession = view.active_session;
    if (view.window_mode) windowMode = view.window_mode;
  }
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = '';
    notice = '';
    try {
      await action();
    } catch (value) {
      error = String(value);
    } finally {
      busy = false;
    }
  }
  async function timerAction(action: string) {
    await run(async () => applyView(await api.action(action)));
  }
  async function sessionAction(action: string) {
    await run(async () => applyView(await api.sessionAction(action)));
  }
  async function setMini(next: boolean) {
    if (busy || modal || isReview || activeSession?.phase === 'review') return;
    await run(async () => {
      windowMode = await api.setPresentationMode(next ? 'mini' : 'normal');
    });
  }
  async function finishFromMini() {
    await run(async () => {
      applyView(await api.action('finish'));
      windowMode = await api.setPresentationMode('normal');
    });
  }
  async function reviewFromMini() {
    await run(async () => {
      windowMode = await api.setPresentationMode('normal');
      if (activeSession?.phase === 'review') modal = 'reviewSession';
    });
  }
  function dragCapsule(event: MouseEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    void api.dragWindow().catch((value) => (error = String(value)));
  }
  function closeModal() {
    if (!busy) {
      modal = null;
      error = '';
    }
  }
  function entry(stint?: Stint) {
    editId = stint?.id ?? null;
    editLabel = stint?.label ?? '';
    editDescription = stint?.description ?? '';
    editDate = stint?.date ?? selected;
    editSessionId = stint?.session_id ?? '';
    originalDuration = stint?.elapsed_ms ?? 1_500_000;
    editMinutes = Math.floor(originalDuration / 60_000);
    editSeconds = Math.floor(originalDuration / 1_000) % 60;
    initialMinutes = editMinutes;
    initialSeconds = editSeconds;
    error = '';
    modal = 'entry';
  }
  function editSession(session: Session) {
    sessionId = session.id;
    sessionLabel = session.label;
    sessionDescription = session.description;
    modal = 'editSession';
  }
  async function saveEntry(event: SubmitEvent) {
    event.preventDefault();
    await run(async () => {
      const elapsed_ms =
        editId &&
        editMinutes === initialMinutes &&
        editSeconds === initialSeconds
          ? originalDuration
          : editMinutes * 60_000 + editSeconds * 1_000;
      await api.edit(editId, {
        label: editLabel,
        description: editDescription,
        date: editDate,
        elapsed_ms,
        session_id: editSessionId || null,
      });
      await refresh();
      selected = editSessionId
        ? sessions.find((session) => session.id === editSessionId)?.date ||
          editDate
        : editDate;
      modal = null;
      notice = editId ? 'Stint updated.' : 'Stint added.';
    });
  }
  function stintsForSession(id: string) {
    return records
      .filter((stint) => stint.session_id === id)
      .sort((a, b) => a.finished_at.localeCompare(b.finished_at));
  }
  function sessionFocus(id: string) {
    return stintsForSession(id).reduce(
      (sum, stint) => sum + stint.elapsed_ms,
      0,
    );
  }
  function changePeriod(delta: number) {
    offset = Math.max(0, offset + delta);
  }
  async function exportData(format: 'csv' | 'json') {
    await run(async () => {
      if (await api.export(format)) {
        modal = null;
        notice = format === 'csv' ? 'CSV exported.' : 'Backup exported.';
      }
    });
  }

  onMount(() => {
    let disposed = false;
    const cleanup: (() => void)[] = [];
    const midnight = setInterval(() => (today = localDate()), 1_000);
    if (!desktop) {
      error =
        'Desktop connection unavailable. Launch cadence with npm run tauri dev.';
      return () => clearInterval(midnight);
    }
    void (async () => {
      try {
        const stateOff = await api.onState((view) => {
          applyView(view);
          if (view.warning && view.warning !== lastWarning) {
            lastWarning = view.warning;
            error = view.warning;
          }
        });
        if (disposed) return stateOff();
        cleanup.push(stateOff);
        const errorOff = await api.onError((message) => (error = message));
        if (disposed) return errorOff();
        cleanup.push(errorOff);
        const modeOff = await api.onWindowMode((mode) => (windowMode = mode));
        if (disposed) return modeOff();
        cleanup.push(modeOff);
        const initial = await api.state();
        applyView(initial);
        if (initial.warning) {
          lastWarning = initial.warning;
          error = initial.warning;
        }
        await refresh();
        ready = true;
      } catch (value) {
        error = String(value);
      }
    })();
    return () => {
      disposed = true;
      clearInterval(midnight);
      cleanup.forEach((fn) => fn());
    };
  });
</script>

<svelte:head><title>cadence</title></svelte:head>
<div class="app-shell" class:mini>
  {#if mini}
    <section
      class="capsule"
      aria-label="cadence mini timer"
    >
      <div
        class="capsule-drag"
        data-tauri-drag-region
        title="Drag capsule"
        onmousedown={dragCapsule}
        aria-hidden="true"><span></span><span></span><span></span></div
      >
      <div class="capsule-readout">
        <span class="capsule-time" role="timer">{clock(
          active
            ? active.target_ms === null
              ? active.elapsed_ms
              : Math.max(0, active.target_ms - active.elapsed_ms)
            : activeSession?.elapsed_ms ?? 0,
        )}</span>
        <span
          class="capsule-label"
          title={active?.label || activeSession?.label || 'Ready'}
          >{active?.phase === 'review'
            ? 'Stint ended'
            : active
              ? active.label || 'Untitled stint'
              : activeSession?.phase === 'review'
                ? 'Session ended'
                : activeSession
                  ? activeSession.label
                  : 'Ready'}</span
        >
      </div>
      <div class="capsule-actions">
        {#if active?.phase === 'review' || activeSession?.phase === 'review'}
          <button
            class="capsule-action capsule-review"
            title="Review ended record"
            aria-label="Review ended record"
            onclick={reviewFromMini}>Review</button
          >
        {:else if active}
          <button
            class="capsule-action"
            title={running ? 'Pause stint' : 'Resume stint'}
            aria-label={running ? 'Pause stint' : 'Resume stint'}
            disabled={busy}
            onclick={() => timerAction(running ? 'pause' : 'resume')}
            >{running ? 'Ⅱ' : '▶'}</button
          ><button
            class="capsule-action capsule-end"
            title="End stint"
            aria-label="End stint"
            disabled={busy}
            onclick={finishFromMini}>■</button
          >
        {:else if activeSession}
          <button
            class="capsule-action"
            title={sessionRunning ? 'Pause session' : 'Resume session'}
            aria-label={sessionRunning ? 'Pause session' : 'Resume session'}
            disabled={busy}
            onclick={() => sessionAction(sessionRunning ? 'pause' : 'resume')}
            >{sessionRunning ? 'Ⅱ' : '▶'}</button
          >
        {/if}
        <button
          class="capsule-action capsule-restore"
          title="Return to normal"
          aria-label="Return to normal"
          onclick={() => setMini(false)}>↗</button
        >
      </div>
    </section>
  {/if}
  <header class="topbar">
    <a
      class="brand"
      href="#timer"
      onclick={(e) => {
        e.preventDefault();
        tab = 'timer';
      }}
      aria-label="cadence timer"
      ><span class="brand-mark" aria-hidden="true">c</span>cadence<span
        class="brand-dot">.</span
      ></a
    >
    <div class="topbar-actions">
      <button
        class="mini-button"
        title="Mini mode"
        aria-label="Mini mode"
        disabled={!!modal || isReview || activeSession?.phase === 'review'}
        onclick={() => setMini(true)}>▰</button
      ><span class="local-badge"><span></span> LOCAL / OFFLINE</span>
    </div>
  </header>
  <nav class="tabs" aria-label="Primary navigation">
    <button
      class:chosen={tab === 'timer'}
      aria-current={tab === 'timer' ? 'page' : undefined}
      onclick={() => (tab = 'timer')}
      ><span class="nav-index">01</span> Timer {#if active || activeSession}<span
          class="tab-dot"
          class:paused={!running && !sessionRunning}
        ></span>{/if}</button
    >
    <button
      class:chosen={tab === 'history'}
      aria-current={tab === 'history' ? 'page' : undefined}
      onclick={() => (tab = 'history')}
      ><span class="nav-index">02</span> History</button
    >
  </nav>
  {#if error && !modal}<div class="message error" role="alert">
      {error}<button aria-label="Dismiss error" onclick={() => (error = '')}
        >×</button
      >
    </div>{/if}
  {#if notice}<div class="message success" role="status">
      {notice}<button aria-label="Dismiss message" onclick={() => (notice = '')}
        >×</button
      >
    </div>{/if}

  <main>
    {#if tab === 'timer'}
      <div class="section-heading">
        <span class="date-stamp"
          >{friendly(today, {
            weekday: 'short',
            day: '2-digit',
            month: 'short',
          }).toUpperCase()}</span
        >{#if !activeSession && !active}<button
            class="text-button"
            onclick={() => {
              sessionLabel = '';
              sessionDescription = '';
              modal = 'startSession';
            }}>+ Start session</button
          >{/if}
      </div>

      {#if activeSession}
        <section
          class="session-panel"
          class:paused={activeSession.phase === 'paused'}
        >
          <div class="session-panel-top">
            <div>
              <span class="eyebrow">ACTIVE SESSION</span>
              <h2>{activeSession.label}</h2>
            </div>
            <strong>{clock(activeSession.elapsed_ms)}</strong>
          </div>
          <div class="session-stats">
            <span
              >{activeSessionStints.length}
              {activeSessionStints.length === 1 ? 'stint' : 'stints'}</span
            ><span>{duration(activeSessionTime)} stint time</span>
          </div>
          {#if activeSession.phase !== 'review'}<div class="session-actions">
              <button
                class="secondary compact"
                disabled={busy}
                onclick={() =>
                  sessionAction(sessionRunning ? 'pause' : 'resume')}
                >{sessionRunning ? 'Pause session' : 'Resume session'}</button
              ><button
                class="secondary compact"
                disabled={busy || !!active}
                onclick={() => sessionAction('end')}>End session</button
              >
            </div>{:else}<button
              class="secondary compact"
              onclick={() => {
                sessionLabel = activeSession?.label ?? '';
                sessionDescription = activeSession?.description ?? '';
                modal = 'reviewSession';
              }}>Review session</button
            >{/if}
        </section>
      {/if}

      {#if isReview && active}
        <section class="review-panel">
          <div class="eyebrow accent">STINT ENDED</div>
          <h1>Stint ended.</h1>
          <div class="review-time">
            {clock(active.elapsed_ms)}<span>STINT TIME</span>
          </div>
          <form
            onsubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await api.save(reviewLabel, description);
                active = null;
                label = '';
                await refresh();
                notice = 'Stint saved.';
              });
            }}
          >
            <label for="review-label">Stint name</label><input
              id="review-label"
              bind:value={reviewLabel}
              placeholder="Required name"
              required
              maxlength="240"
              disabled={busy}
            />
            <label for="review-description"
              >Notes <span class="optional">optional</span></label
            ><textarea
              id="review-description"
              bind:value={description}
              placeholder="Optional notes"
              rows="2"
              maxlength="20000"
              disabled={busy}></textarea>
            <button class="primary full" disabled={busy || !reviewLabel.trim()}
              >Save stint <span aria-hidden="true">↗</span></button
            >
            <button
              type="button"
              class="quiet full"
              disabled={busy}
              onclick={() => (modal = 'discardStint')}>Discard stint</button
            >
          </form>
        </section>
      {:else}
        <div class="mode-switch" aria-label="Timer mode">
          <button
            class:chosen={(active?.mode ?? mode) === 'count_up'}
            disabled={!!active}
            aria-pressed={(active?.mode ?? mode) === 'count_up'}
            onclick={() => (mode = 'count_up')}>Count up <span>↗</span></button
          >
          <button
            class:chosen={(active?.mode ?? mode) === 'countdown'}
            disabled={!!active}
            aria-pressed={(active?.mode ?? mode) === 'countdown'}
            onclick={() => (mode = 'countdown')}
            >Countdown <span>↘</span></button
          >
        </div>
        <section
          class="timer-panel"
          class:is-running={running}
          aria-label="Stint timer"
        >
          <div class="timer-status">
            <span class="status-dot" class:live={running}></span>{active
              ? running
                ? 'RUNNING'
                : 'PAUSED'
              : 'NO ACTIVE STINT'}
          </div>
          <div
            class="timer-digits"
            class:long={displayTime >= 360_000_000}
            role="timer"
            aria-label={`${active?.target_ms || (!active && mode === 'countdown') ? 'Remaining' : 'Elapsed'} time ${clock(displayTime)}`}
          >
            {clock(displayTime)}
          </div>
          <div class="timer-caption">
            {active
              ? active.mode === 'countdown'
                ? 'remaining'
                : 'elapsed'
              : activeSession
                ? `next stint in ${activeSession.label}`
                : 'standalone stint'}
          </div>
          <div class="track" aria-hidden="true">
            <span
              style:width={active?.target_ms
                ? `${progress}%`
                : running
                  ? '33%'
                  : '0%'}
            ></span><i></i><i></i><i></i><i></i><i></i>
          </div>
          <div class="track-labels">
            <span>{active ? 'ACTIVE STINT' : 'TIMER READY'}</span><span
              >{active?.target_ms ? `${Math.round(progress)}%` : '— —'}</span
            >
          </div>
        </section>
        {#if !active}
          <form
            class="start-form"
            onsubmit={(e) => {
              e.preventDefault();
              void run(async () =>
                applyView(
                  await api.start(
                    mode,
                    label,
                    mode === 'countdown' ? minutes : null,
                  ),
                ),
              );
            }}
          >
            <label for="label"
              >Stint label <span class="optional">optional until save</span
              ></label
            ><input
              id="label"
              bind:value={label}
              placeholder="Optional stint label"
              maxlength="240"
              disabled={busy ||
                (activeSession?.phase !== 'running' && !!activeSession)}
            />
            {#if mode === 'countdown'}<div class="duration-setting">
                <label for="minutes">Target duration</label>
                <div>
                  <input
                    id="minutes"
                    type="number"
                    min="1"
                    max="5256000"
                    step="1"
                    required
                    bind:value={minutes}
                  /><span>min</span>
                </div>
              </div>{/if}
            <button
              class="primary full"
              disabled={busy ||
                !ready ||
                (activeSession?.phase !== 'running' && !!activeSession)}
              >Start stint <span aria-hidden="true">↗</span></button
            >
          </form>
        {:else}
          <div class="active-label">
            <span class="eyebrow">STINT LABEL</span>
            <p>{active.label || 'Untitled stint'}</p>
          </div>
          <div class="timer-actions">
            <button
              class="secondary"
              disabled={busy}
              onclick={() => timerAction(running ? 'pause' : 'resume')}
              >{running ? 'Ⅱ Pause' : '↗ Resume'}</button
            ><button
              class="primary"
              disabled={busy}
              onclick={() => timerAction('finish')}
              >End stint <span aria-hidden="true">↗</span></button
            >
          </div>
          <p class="background-hint">
            {running
              ? 'Timing continues when the window is closed.'
              : 'Paused time is excluded.'}
          </p>
        {/if}
      {/if}
      <section class="today-summary" aria-label="Today’s activity">
        <div>
          <span class="eyebrow">TODAY</span>
          <div>
            <strong>{todaySummary.count.toString().padStart(2, '0')}</strong
            ><span>stints</span>
          </div>
        </div>
        <div class="today-duration">
          <strong>{duration(todaySummary.elapsed_ms)}</strong><span
            >stint time</span
          >
        </div>
      </section>
    {:else}
      <div class="history-heading">
        <h1>History</h1>
        <button
          class="icon-button"
          aria-label="Export and backup"
          title="Export and backup"
          onclick={() => {
            error = '';
            modal = 'data';
          }}>↥</button
        >
      </div>
      <section class="activity" aria-label="Activity calendar">
        <div class="calendar-toolbar">
          <span
            >{friendly(days[0])} — {friendly(days[83], {
              month: 'short',
              day: 'numeric',
              year: 'numeric',
            })}</span
          >
          <div>
            <button
              class="icon-button"
              aria-label="Previous twelve weeks"
              onclick={() => changePeriod(1)}>‹</button
            ><button
              class="icon-button"
              aria-label="Next twelve weeks"
              disabled={offset === 0}
              onclick={() => changePeriod(-1)}>›</button
            >
          </div>
        </div>
        <div class="calendar-body">
          <div class="week-labels" aria-hidden="true">
            <span>M</span><span></span><span>W</span><span></span><span>F</span
            ><span></span><span>S</span>
          </div>
          <div class="heatmap">
            {#each days as day}<button
                class="day level-{Math.min(summaries[day]?.count ?? 0, 4)}"
                class:selected={day === selected}
                class:future={day > today}
                aria-pressed={day === selected}
                aria-label={`${friendly(day, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' })}: ${summaries[day]?.count ?? 0} stints`}
                title={`${day} · ${summaries[day]?.count ?? 0} stints`}
                onclick={() => (selected = day)}
              ></button>{/each}
          </div>
        </div>
        <div class="calendar-footer">
          <button
            class="text-button"
            onclick={() => {
              offset = 0;
              selected = today;
            }}>Today</button
          >
          <div
            class="legend"
            aria-label="Intensity: zero, one, two, three, four or more stints"
          >
            <span>0</span>{#each [0, 1, 2, 3, 4] as n}<i class="level-{n}"
              ></i>{/each}<span>4+</span>
          </div>
        </div>
      </section>
      <section class="day-detail">
        <div class="day-heading">
          <div>
            <h2>
              {selected === today
                ? 'Today'
                : friendly(selected, { month: 'long', day: 'numeric' })}
            </h2>
            <span
              >{selectedSummary.count}
              {selectedSummary.count === 1 ? 'stint' : 'stints'} · {selectedSummary.session_count}
              {selectedSummary.session_count === 1 ? 'session' : 'sessions'} · {duration(
                selectedSummary.elapsed_ms,
              )}</span
            >
          </div>
          <button
            class="add-button"
            disabled={!ready || busy}
            onclick={() => entry()}>+ Add stint</button
          >
        </div>
        {#if selectedSessions.length === 0 && selectedStandalone.length === 0}<div
            class="empty-state"
          >
            <h3>No records.</h3>
            <p>No stints or sessions recorded for this date.</p>
          </div>{:else}
          <div class="history-records">
            {#each selectedSessions as session}
              <article class="session-card">
                <header>
                  <button
                    class="session-title"
                    onclick={() => editSession(session)}
                    aria-label={`Edit session ${session.label}`}
                    ><span class="eyebrow">SESSION</span><strong
                      >{session.label}</strong
                    ></button
                  >
                  <div>
                    <span>{clock(session.elapsed_ms)}</span><button
                      class="delete-button"
                      aria-label={`Delete session ${session.label}`}
                      onclick={() => {
                        sessionId = session.id;
                        sessionLabel = session.label;
                        modal = 'deleteSession';
                      }}>×</button
                    >
                  </div>
                </header>
                <div class="session-card-stats">
                  <span>{stintsForSession(session.id).length} stints</span><span
                    >{duration(sessionFocus(session.id))} stint time</span
                  >
                </div>
                {#if session.description}<p class="session-notes">
                    {session.description}
                  </p>{/if}
                {#if stintsForSession(session.id).length === 0}<p
                    class="empty-inline"
                  >
                    No stints in this session.
                  </p>{:else}<ul class="nested-stints">
                    {#each stintsForSession(session.id) as stint}<li>
                        <button
                          class="stint-content"
                          onclick={() => entry(stint)}
                          aria-label={`Edit ${stint.label}`}
                          ><div class="stint-top">
                            <strong>{stint.label}</strong><span
                              >{duration(stint.elapsed_ms)}</span
                            >
                          </div>
                          {#if stint.description}<p>
                              {stint.description}
                            </p>{/if}<span class="stint-meta"
                            >{stint.mode === 'manual'
                              ? 'MANUAL'
                              : stint.mode === 'countdown'
                                ? 'COUNTDOWN'
                                : 'COUNT UP'} · EDIT</span
                          ></button
                        ><button
                          class="delete-button"
                          aria-label={`Delete ${stint.label}`}
                          onclick={() => {
                            deleteId = stint.id;
                            deleteLabel = stint.label;
                            modal = 'deleteStint';
                          }}>×</button
                        >
                      </li>{/each}
                  </ul>{/if}
              </article>
            {/each}
            {#if selectedStandalone.length}<div class="standalone-heading">
                STANDALONE STINTS
              </div>
              <ul class="stint-list">
                {#each selectedStandalone as stint}<li>
                    <button
                      class="stint-content"
                      onclick={() => entry(stint)}
                      aria-label={`Edit ${stint.label}`}
                      ><div class="stint-top">
                        <strong>{stint.label}</strong><span
                          >{duration(stint.elapsed_ms)}</span
                        >
                      </div>
                      {#if stint.description}<p>
                          {stint.description}
                        </p>{/if}<span class="stint-meta"
                        >{stint.mode === 'manual'
                          ? 'MANUAL'
                          : stint.mode === 'countdown'
                            ? 'COUNTDOWN'
                            : 'COUNT UP'} · EDIT</span
                      ></button
                    ><button
                      class="delete-button"
                      aria-label={`Delete ${stint.label}`}
                      onclick={() => {
                        deleteId = stint.id;
                        deleteLabel = stint.label;
                        modal = 'deleteStint';
                      }}>×</button
                    >
                  </li>{/each}
              </ul>{/if}
          </div>
        {/if}
      </section>
    {/if}
  </main>
  <footer><span>LOCAL DATA</span><span>cadence / 0.2</span></footer>
</div>

<dialog
  bind:this={dialog}
  oncancel={(e) => {
    e.preventDefault();
    closeModal();
  }}
  aria-labelledby="dialog-title"
>
  <div class="dialog-header">
    <span class="eyebrow"
      >CADENCE / {modal
        ?.replace(/[A-Z]/g, (letter) => ` ${letter}`)
        .toUpperCase()}</span
    ><button
      class="icon-button"
      aria-label="Close dialog"
      disabled={busy}
      onclick={closeModal}>×</button
    >
  </div>
  {#if error}<div class="message error" role="alert">{error}</div>{/if}
  {#if modal === 'entry'}
    <h2 id="dialog-title">{editId ? 'Edit stint' : 'Add stint'}</h2>
    <form onsubmit={saveEntry}>
      <label for="edit-label">Name</label><input
        id="edit-label"
        required
        maxlength="240"
        bind:value={editLabel}
      />
      <label for="edit-date">Original date</label><input
        id="edit-date"
        type="date"
        required
        bind:value={editDate}
      />
      <label for="edit-session">Session</label><select
        id="edit-session"
        bind:value={editSessionId}
        ><option value="">Standalone</option>{#each sessions as session}<option
            value={session.id}>{session.label} — {session.date}</option
          >{/each}</select
      >
      <div class="duration-fields">
        <div>
          <label for="edit-minutes">Minutes</label><input
            id="edit-minutes"
            type="number"
            min="0"
            max="5256000"
            step="1"
            required
            bind:value={editMinutes}
          />
        </div>
        <div>
          <label for="edit-seconds">Seconds</label><input
            id="edit-seconds"
            type="number"
            min="0"
            max="59"
            step="1"
            required
            bind:value={editSeconds}
          />
        </div>
      </div>
      <label for="edit-description"
        >Notes <span class="optional">optional</span></label
      ><textarea
        id="edit-description"
        rows="3"
        maxlength="20000"
        bind:value={editDescription}></textarea>
      <button
        class="primary full"
        disabled={busy ||
          !editLabel.trim() ||
          editMinutes * 60 + editSeconds <= 0}
        >Save stint <span aria-hidden="true">↗</span></button
      >
    </form>
  {:else if modal === 'startSession'}
    <h2 id="dialog-title">Start session</h2>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void run(async () => {
          applyView(
            await api.startSession({
              label: sessionLabel,
              description: sessionDescription,
            }),
          );
          modal = null;
        });
      }}
    >
      <label for="session-name">Session name</label><input
        id="session-name"
        required
        maxlength="240"
        bind:value={sessionLabel}
      /><label for="session-notes"
        >Notes <span class="optional">optional</span></label
      ><textarea
        id="session-notes"
        maxlength="20000"
        rows="3"
        bind:value={sessionDescription}></textarea><button
        class="primary full"
        disabled={busy || !sessionLabel.trim()}
        >Start session <span>↗</span></button
      >
    </form>
  {:else if modal === 'reviewSession'}
    <h2 id="dialog-title">Session ended</h2>
    <div class="review-summary">
      <strong
        >{activeSession ? clock(activeSession.elapsed_ms) : '00:00:00'}</strong
      ><span
        >{activeSessionStints.length} stints · {duration(activeSessionTime)} stint
        time</span
      >
    </div>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void run(async () => {
          await api.saveSession({
            label: sessionLabel,
            description: sessionDescription,
          });
          activeSession = null;
          await refresh();
          modal = null;
          notice = 'Session saved.';
        });
      }}
    >
      <label for="review-session-name">Session name</label><input
        id="review-session-name"
        required
        maxlength="240"
        bind:value={sessionLabel}
      /><label for="review-session-notes"
        >Notes <span class="optional">optional</span></label
      ><textarea
        id="review-session-notes"
        maxlength="20000"
        rows="3"
        bind:value={sessionDescription}></textarea><button
        class="primary full"
        disabled={busy || !sessionLabel.trim()}
        >Save session <span>↗</span></button
      ><button
        type="button"
        class="quiet full"
        onclick={() => (modal = 'discardSession')}>Discard session</button
      >
    </form>
  {:else if modal === 'editSession'}
    <h2 id="dialog-title">Edit session</h2>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void run(async () => {
          await api.editSession(sessionId, {
            label: sessionLabel,
            description: sessionDescription,
          });
          await refresh();
          modal = null;
          notice = 'Session updated.';
        });
      }}
    >
      <label for="edit-session-name">Session name</label><input
        id="edit-session-name"
        required
        maxlength="240"
        bind:value={sessionLabel}
      /><label for="edit-session-notes"
        >Notes <span class="optional">optional</span></label
      ><textarea
        id="edit-session-notes"
        maxlength="20000"
        rows="3"
        bind:value={sessionDescription}></textarea><button
        class="primary full"
        disabled={busy || !sessionLabel.trim()}
        >Save session <span>↗</span></button
      >
    </form>
  {:else if modal === 'discardStint' || modal === 'deleteStint' || modal === 'deleteSession' || modal === 'discardSession'}
    <h2 id="dialog-title">
      {modal === 'discardStint'
        ? 'Discard stint?'
        : modal === 'deleteStint'
          ? 'Delete stint?'
          : modal === 'deleteSession'
            ? 'Delete session?'
            : 'Discard session?'}
    </h2>
    <p class="dialog-copy">
      {modal === 'discardStint'
        ? 'The ended stint will not be saved.'
        : modal === 'deleteStint'
          ? `“${deleteLabel}” will be deleted.`
          : modal === 'deleteSession'
            ? `“${sessionLabel}” will be deleted. Its stints will remain as standalone records.`
            : 'The session will be deleted. Its saved stints will remain as standalone records.'}
    </p>
    <div class="dialog-actions">
      <button class="secondary" disabled={busy} onclick={closeModal}
        >Cancel</button
      ><button
        class="primary"
        disabled={busy}
        onclick={() =>
          run(async () => {
            if (modal === 'discardStint')
              applyView(await api.action('discard'));
            else if (modal === 'deleteStint') await api.delete(deleteId);
            else if (modal === 'deleteSession')
              await api.deleteSession(sessionId);
            else applyView(await api.sessionAction('discard'));
            await refresh();
            modal = null;
          })}>{modal?.startsWith('discard') ? 'Discard' : 'Delete'}</button
      >
    </div>
  {:else if modal === 'data'}
    <h2 id="dialog-title">Data</h2>
    <p class="dialog-copy">Stints and sessions are stored locally.</p>
    <button
      class="data-option"
      disabled={busy}
      onclick={() => exportData('csv')}
      ><span
        ><strong>Export CSV</strong><small
          >Export sessions and stints as rows.</small
        ></span
      ><span>↗</span></button
    >
    <button
      class="data-option"
      disabled={busy}
      onclick={() => exportData('json')}
      ><span
        ><strong>Export backup</strong><small
          >Create a restorable JSON backup.</small
        ></span
      ><span>↗</span></button
    >
    <button
      class="data-option"
      disabled={busy}
      onclick={() =>
        run(async () => {
          preview = await api.preview();
          if (preview) modal = 'import';
        })}
      ><span
        ><strong>Import backup</strong><small
          >Validate and merge a cadence backup.</small
        ></span
      ><span>↙</span></button
    >
    <p class="fine-print">
      Active timing is excluded. End and save active records before exporting.
    </p>
  {:else if modal === 'import' && preview}
    <h2 id="dialog-title">Import preview</h2>
    <p class="dialog-copy">
      {preview.total} records checked.<br /><strong
        >{preview.added} records</strong
      >
      will be added.<br />{preview.skipped} identical records will be skipped.
    </p>
    <div class="dialog-actions">
      <button class="secondary" disabled={busy} onclick={closeModal}
        >Cancel</button
      ><button
        class="primary"
        disabled={busy}
        onclick={() =>
          run(async () => {
            const result = await api.import();
            await refresh();
            modal = null;
            notice = `${result.added} records imported.`;
          })}>Import</button
      >
    </div>
  {/if}
</dialog>
