<script lang="ts">
  import {
    transferAnswer,
    transferCancel,
    transferCancelAll,
    transferClear,
    type OverwriteChoice,
    type TransferJob,
  } from "../backend/files";
  import Icon from "../ui/Icon.svelte";
  import { formatSize, percent } from "./paths";

  interface Props {
    jobs: TransferJob[];
  }

  let { jobs }: Props = $props();

  // Folder-making steps are plumbing: only files are shown.
  const shown = $derived(jobs.filter((j) => j.kind === "copy"));
  const asking = $derived(jobs.find((j) => j.state.state === "asking"));
  const active = $derived(shown.filter((j) => ["queued", "asking", "running"].includes(j.state.state)));
  const finished = $derived(jobs.filter((j) => !["queued", "asking", "running"].includes(j.state.state)));
  const failures = $derived(jobs.filter((j) => j.state.state === "failed").length);

  function label(job: TransferJob): string {
    switch (job.state.state) {
      case "queued":
        return "Waiting";
      case "asking":
        return "Needs your answer";
      case "running": {
        const p = percent(job.done, job.total);
        return p === null ? formatSize(job.done) : `${p}%`;
      }
      case "done":
        return "Done";
      case "skipped":
        return "Skipped";
      case "cancelled":
        return "Cancelled";
      case "failed":
        return job.state.error.message;
    }
  }

  function answer(choice: OverwriteChoice) {
    if (asking) void transferAnswer(asking.id, choice).catch(() => {});
  }
</script>

{#if shown.length > 0}
  <section class="transfers">
    <header>
      <span class="title">Transfers</span>
      <span class="summary">
        {#if active.length}{active.length} in progress{:else if failures}{failures} failed{:else}All done{/if}
      </span>
      {#if active.length}
        <button class="btn small ghost" onclick={() => transferCancelAll().catch(() => {})}>Cancel all</button>
      {/if}
      {#if finished.length}
        <button class="btn small ghost" onclick={() => transferClear().catch(() => {})}>Clear</button>
      {/if}
    </header>

    <ul>
      {#each shown as job (job.id)}
        {@const state = job.state.state}
        {@const p = percent(job.done, job.total)}
        <li class:failed={state === "failed"} class:done={state === "done"}>
          <div class="line">
            <span class="name" title={`${job.from} → ${job.to}`}>{job.name}</span>
            <span class="status" class:bad={state === "failed"}>{label(job)}</span>
            {#if state === "queued" || state === "running" || state === "asking"}
              <button class="icon-btn small" title="Cancel" onclick={() => transferCancel(job.id).catch(() => {})}>
                <Icon name="close" size={12} />
              </button>
            {:else if state === "done"}
              <span class="ok"><Icon name="check" size={13} /></span>
            {/if}
          </div>
          {#if state === "running"}
            <div class="bar" role="progressbar" aria-valuenow={p ?? undefined} aria-valuemin="0" aria-valuemax="100">
              <span class:indeterminate={p === null} style:width={p === null ? "40%" : `${p}%`}></span>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}

{#if asking}
  <div class="modal-backdrop">
    <div class="modal" role="alertdialog" aria-label="File already exists">
      <h2>Replace this file?</h2>
      <p class="lead"><b>{asking.name}</b> already exists in the destination.</p>
      <div class="choices">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn" autofocus onclick={() => answer("skip")}>Skip</button>
        <button class="btn danger" onclick={() => answer("overwrite")}>Replace</button>
      </div>
      <div class="all">
        <button class="btn small ghost" onclick={() => answer("skip_all")}>Skip all that exist</button>
        <button class="btn small ghost" onclick={() => answer("overwrite_all")}>Replace all that exist</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .transfers {
    border-top: 1px solid var(--border);
    background: var(--bg-1);
    max-height: 34%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 4px 12px;
  }
  .title {
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .summary {
    flex: 1;
    color: var(--text-faint);
    font-size: 12px;
  }
  ul {
    margin: 0;
    padding: 0 10px 8px;
    list-style: none;
    overflow: auto;
  }
  li {
    padding: 5px 0;
    border-bottom: 1px solid var(--border);
  }
  li:last-child {
    border-bottom: none;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status {
    flex: none;
    max-width: 55%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-faint);
    font-size: 12px;
  }
  .status.bad {
    color: var(--danger);
  }
  .ok {
    display: grid;
    color: var(--success);
  }
  .bar {
    height: 4px;
    margin-top: 5px;
    overflow: hidden;
    background: var(--bg-3);
    border-radius: 4px;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 4px;
    transition: width 0.15s linear;
  }
  .bar span.indeterminate {
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      margin-left: -40%;
    }
    to {
      margin-left: 100%;
    }
  }
  .choices {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
  .all {
    display: flex;
    justify-content: flex-end;
    gap: 4px;
    margin-top: 8px;
  }
</style>
