<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import Curve from "$lib/Curve.svelte";
  import {
    getStatus,
    getIndex,
    getCurve,
    applyProfile,
    clearProfile,
    setMaster,
    setBassShelf,
    setHeadroom,
    setCompensateShelf,
    setAutostart,
    setupEapoInclude,
    openConfigurator,
    groupIndex,
    searchGroups,
    type AppStatus,
    type CurveData,
    type DeviceView,
    type GroupedHeadphone,
    type HeadphoneEntry,
  } from "$lib/api";

  let status = $state<AppStatus | null>(null);
  let groups = $state<GroupedHeadphone[]>([]);
  let indexAge = $state<number | null>(null);
  let loadError = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let busy = $state(false);
  let indexLoading = $state(false);

  let selectedGuid = $state<string | null>(null);
  let query = $state("");
  let expandedResult = $state<string | null>(null);

  const results = $derived(searchGroups(groups, query));
  const selectedDevice = $derived(
    status?.devices.find((d) => d.guid === selectedGuid) ??
      status?.devices.find((d) => d.is_default) ??
      status?.devices[0] ??
      null,
  );

  let curveData = $state<CurveData | null>(null);
  let curveFor = "";
  $effect(() => {
    const a = selectedDevice?.assignment;
    const guid = selectedDevice?.guid;
    if (!a || !guid) {
      curveData = null;
      curveFor = "";
      return;
    }
    const key = `${guid}|${a.source}|${a.name}`;
    if (key === curveFor) return;
    curveFor = key;
    getCurve(guid)
      .then((c) => (curveData = c))
      .catch(() => (curveData = null));
  });

  async function refreshStatus() {
    try {
      status = await getStatus();
      loadError = null;
      if (!selectedGuid && status.devices.length) {
        selectedGuid = (status.devices.find((d) => d.is_default) ?? status.devices[0]).guid;
      }
    } catch (e) {
      loadError = String(e);
    }
  }

  async function loadIndex(force = false) {
    indexLoading = true;
    try {
      const cache = await getIndex(force);
      groups = groupIndex(cache.entries);
      indexAge = cache.fetched_at;
      actionError = null;
    } catch (e) {
      actionError = `Could not load the AutoEq database: ${e}`;
    } finally {
      indexLoading = false;
    }
  }

  let pendingUpdate = $state<Update | null>(null);
  let updating = $state(false);

  async function checkForUpdate() {
    try {
      pendingUpdate = await check();
    } catch {
      // Offline or no releases yet — silently skip.
    }
  }

  async function installUpdate() {
    if (!pendingUpdate) return;
    updating = true;
    try {
      await pendingUpdate.downloadAndInstall();
      await relaunch();
    } catch (e) {
      actionError = `Update failed: ${e}`;
      updating = false;
    }
  }

  onMount(() => {
    refreshStatus();
    loadIndex();
    checkForUpdate();
    const unlisten = listen("status-changed", refreshStatus);
    return () => {
      unlisten.then((fn) => fn());
    };
  });

  async function onApply(entry: HeadphoneEntry) {
    if (!selectedDevice) return;
    busy = true;
    actionError = null;
    try {
      status = await applyProfile(selectedDevice.guid, selectedDevice.name, entry);
      query = "";
      expandedResult = null;
    } catch (e) {
      actionError = String(e);
    } finally {
      busy = false;
    }
  }

  async function onClear(guid: string) {
    busy = true;
    actionError = null;
    try {
      status = await clearProfile(guid);
    } catch (e) {
      actionError = String(e);
    } finally {
      busy = false;
    }
  }

  async function onToggleMaster() {
    if (!status) return;
    busy = true;
    try {
      status = await setMaster(!status.master_enabled);
    } catch (e) {
      actionError = String(e);
    } finally {
      busy = false;
    }
  }

  async function onToggleAutostart() {
    if (!status) return;
    try {
      const enabled = await setAutostart(!status.autostart);
      status = { ...status, autostart: enabled };
    } catch (e) {
      actionError = String(e);
    }
  }

  async function onSetupInclude() {
    busy = true;
    try {
      status = await setupEapoInclude();
    } catch (e) {
      actionError = String(e);
    } finally {
      busy = false;
    }
  }

  let headroomDraft = $state<number | null>(null);
  let headroomTimer: ReturnType<typeof setTimeout> | undefined;
  const headroomShown = $derived(headroomDraft ?? status?.headroom_db ?? -3);

  function onHeadroomInput(e: Event) {
    const v = Number((e.target as HTMLInputElement).value);
    headroomDraft = v;
    clearTimeout(headroomTimer);
    headroomTimer = setTimeout(async () => {
      try {
        status = await setHeadroom(v);
      } catch (err) {
        actionError = String(err);
      } finally {
        headroomDraft = null;
      }
    }, 250);
  }

  let bassDraft = $state<{ gain: number; fc: number } | null>(null);
  let bassTimer: ReturnType<typeof setTimeout> | undefined;
  const bassShown = $derived(
    bassDraft ?? {
      gain: selectedDevice?.assignment?.bass_gain_db ?? 0,
      fc: selectedDevice?.assignment?.bass_fc ?? 350,
    },
  );

  function onBassInput(field: "gain" | "fc", e: Event) {
    if (!selectedDevice?.assignment) return;
    const v = Number((e.target as HTMLInputElement).value);
    const next = { ...bassShown, [field]: v };
    bassDraft = next;
    const guid = selectedDevice.guid;
    clearTimeout(bassTimer);
    bassTimer = setTimeout(async () => {
      try {
        status = await setBassShelf(guid, next.gain, next.fc);
      } catch (err) {
        actionError = String(err);
      } finally {
        bassDraft = null;
      }
    }, 250);
  }

  let seriesVisible = $state({ raw: true, target: true, shelf: true, main: true });
  let smoothedView = $state(true);
  const rawSeries = $derived.by(() => {
    if (!curveData) return [] as [number, number][];
    if (smoothedView && curveData.smoothed.length) return curveData.smoothed;
    return curveData.raw;
  });

  function convolutionActive(d: DeviceView): boolean {
    return (
      !!d.assignment?.has_ir && (d.sample_rate === 44100 || d.sample_rate === 48000)
    );
  }
</script>

<main>
  <header>
    <div class="title">
      <h1>Impulse</h1>
      <span class="subtitle">AutoEq for your system audio</span>
    </div>
    {#if status}
      <button
        class="master-toggle"
        class:on={status.master_enabled}
        onclick={onToggleMaster}
        disabled={busy || !status.eapo.installed}
        title="Enable or disable all EQ"
      >
        <span class="knob"></span>
        <span class="label">{status.master_enabled ? "EQ on" : "EQ off"}</span>
      </button>
    {/if}
  </header>

  {#if pendingUpdate}
    <div class="banner update">
      <strong>Update available:</strong> Impulse {pendingUpdate.version}
      <button onclick={installUpdate} disabled={updating}>
        {updating ? "Installing…" : "Install & restart"}
      </button>
    </div>
  {/if}

  {#if loadError}
    <div class="banner error">{loadError}</div>
  {/if}

  {#if status && !status.eapo.installed}
    <div class="banner warn">
      <strong>Equalizer APO is not installed.</strong>
      <p>
        WinAutoConv uses Equalizer APO as its processing backend. Install it once, enable it on
        your playback device, and reboot — after that one-time step, all profile changes from
        here apply instantly.
      </p>
      <button onclick={() => openUrl("https://sourceforge.net/projects/equalizerapo/")}>
        Download Equalizer APO
      </button>
    </div>
  {:else if status && !status.eapo.include_installed}
    <div class="banner warn">
      <strong>One step left:</strong> hook WinAutoConv into Equalizer APO's config.
      <button onclick={onSetupInclude} disabled={busy}>Set up now</button>
    </div>
  {/if}

  {#if status}
    <section class="card">
      <h2>Output device</h2>
      <select
        class="device-select"
        value={selectedDevice?.guid}
        onchange={(e) => (selectedGuid = (e.target as HTMLSelectElement).value)}
      >
        {#each status.devices as d (d.guid)}
          <option value={d.guid}>{d.name}{d.is_default ? "  (default)" : ""}</option>
        {/each}
      </select>
      {#if selectedDevice}
        <div class="device-meta">
          {#if selectedDevice.is_default}<span class="chip">default</span>{/if}
          <span>{(selectedDevice.sample_rate / 1000).toFixed(1)} kHz</span>
          {#if status.eapo.installed}
            {#if selectedDevice.apo_enabled}
              <span class="chip ok">APO active</span>
            {:else}
              <span class="chip bad">APO not enabled</span>
            {/if}
          {/if}
          {#if selectedDevice.assignment}
            <span class="device-profile-inline">
              <span class="profile-name">{selectedDevice.assignment.name}</span>
              <span class="dim"
                >{selectedDevice.assignment.source} · {convolutionActive(selectedDevice)
                  ? "convolution"
                  : "graphic EQ"}</span
              >
            </span>
          {:else}
            <span class="dim">no EQ profile</span>
          {/if}
        </div>
      {/if}
      {#if selectedDevice && !selectedDevice.apo_enabled && status.eapo.installed}
        <div class="hint">
          Equalizer APO isn't enabled on “{selectedDevice.name}”. Enable it in its device
          selector (one-time, needs an audio restart), then profiles applied here will work.
          <button class="link" onclick={() => openConfigurator().catch((e) => (actionError = String(e)))}>
            Open device selector
          </button>
        </div>
      {/if}
    </section>

    <section class="card">
      <h2>
        Headphone profile
        {#if selectedDevice}<span class="for-device">for {selectedDevice.name}</span>{/if}
      </h2>
      <input
        type="search"
        placeholder={indexLoading
          ? "Loading AutoEq database…"
          : `Search ${groups.length.toLocaleString()} headphones…`}
        bind:value={query}
        disabled={indexLoading || !status.eapo.installed}
      />
      {#if query && results.length === 0 && !indexLoading}
        <div class="hint">No matches. Try fewer words.</div>
      {/if}
      {#if results.length}
        <ul class="results">
          {#each results as g (g.name)}
            <li>
              <div class="result-row">
                <div class="result-info">
                  <span class="result-name">{g.name}</span>
                  <span class="result-sub">
                    {g.variants[0].source} · {g.variants[0].form}
                    {#if g.variants.length > 1}
                      · <button
                        class="link"
                        onclick={() => (expandedResult = expandedResult === g.name ? null : g.name)}
                        >{g.variants.length} sources</button
                      >
                    {/if}
                  </span>
                </div>
                <button class="apply" disabled={busy || !selectedDevice} onclick={() => onApply(g.variants[0])}>
                  Apply
                </button>
              </div>
              {#if expandedResult === g.name}
                <ul class="variants">
                  {#each g.variants as v (v.source + v.form)}
                    <li>
                      <span>{v.source} · {v.form} {v.has_ir ? "" : "(no IR)"}</span>
                      <button class="apply small" disabled={busy || !selectedDevice} onclick={() => onApply(v)}>
                        Apply
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if selectedDevice?.assignment}
      <section class="card">
        <div class="response-head">
          <h2>
            Response
            <span class="for-device">{selectedDevice.assignment.name} on {selectedDevice.name}</span>
          </h2>
          <button class="remove" disabled={busy} onclick={() => onClear(selectedDevice.guid)}>
            Remove profile
          </button>
        </div>
        {#if curveData}
          <Curve
            points={curveData.eq}
            raw={rawSeries}
            target={curveData.target}
            bassGain={bassShown.gain}
            bassFc={bassShown.fc}
            show={seriesVisible}
          />
          <div class="legend">
            {#if rawSeries.length}
              <button
                class="legend-item"
                class:off={!seriesVisible.raw}
                onclick={() => (seriesVisible.raw = !seriesVisible.raw)}
                ><i class="swatch raw"></i>Raw</button
              >
            {/if}
            {#if curveData.target.length}
              <button
                class="legend-item"
                class:off={!seriesVisible.target}
                onclick={() => (seriesVisible.target = !seriesVisible.target)}
                ><i class="swatch target"></i>Target</button
              >
            {/if}
            {#if bassShown.gain !== 0}
              <button
                class="legend-item"
                class:off={!seriesVisible.shelf}
                onclick={() => (seriesVisible.shelf = !seriesVisible.shelf)}
                ><i class="swatch shelf"></i>Bass shelf</button
              >
            {/if}
            <button
              class="legend-item"
              class:off={!seriesVisible.main}
              onclick={() => (seriesVisible.main = !seriesVisible.main)}
              ><i class="swatch total"></i>{rawSeries.length ? "Corrected" : "Correction"}
              ({curveData.eq_source === "convolver" ? "measured IR" : "graphic EQ"})</button
            >
            <span class="legend-spacer"></span>
            {#if curveData.smoothed.length}
              <button
                class="legend-item"
                class:off={!smoothedView}
                onclick={() => (smoothedView = !smoothedView)}>Smoothed</button
              >
            {/if}
          </div>
          {#if curveData.eq_source === "convolver"}
            <div class="hint">
              Correction measured from the active convolver IR
              ({curveData.ir_preamp_db.toFixed(1)} dB built-in gain, shown level-aligned).
            </div>
          {/if}
        {/if}
        <div class="switch-row">
          {#if status.compensate_shelf && bassShown.gain > 0}
            <span class="dim">(volume: {(headroomShown - bassShown.gain).toFixed(1)} dB)</span>
          {/if}
          <span class="switch-spacer"></span>
          <span title="Automatically lowers the pre-gain by the bass boost amount so boosted bass can't clip. Off = you manage headroom yourself.">
            Prevent distortion from bass boost
          </span>
          <button
            class="switch"
            class:on={status.compensate_shelf}
            role="switch"
            aria-checked={status.compensate_shelf}
            aria-label="Prevent distortion from bass boost"
            onclick={async () => {
              try {
                status = await setCompensateShelf(!status!.compensate_shelf);
              } catch (e) {
                actionError = String(e);
              }
            }}
          >
            <span class="switch-knob"></span>
          </button>
        </div>
        <div class="sliders">
          <div class="slider-row">
            <span class="slider-label">Bass gain</span>
            <input
              type="range"
              min="-12"
              max="12"
              step="0.5"
              value={bassShown.gain}
              oninput={(e) => onBassInput("gain", e)}
            />
            <span class="slider-value">{bassShown.gain > 0 ? "+" : ""}{bassShown.gain.toFixed(1)} dB</span>
          </div>
          <div class="slider-row">
            <span class="slider-label">Corner</span>
            <input
              type="range"
              min="50"
              max="350"
              step="5"
              value={bassShown.fc}
              oninput={(e) => onBassInput("fc", e)}
            />
            <span class="slider-value">{bassShown.fc.toFixed(0)} Hz</span>
          </div>
        </div>
        {#if bassShown.gain > 0 && !status.compensate_shelf}
          <div class="hint">
            Distortion prevention is off — this +{bassShown.gain.toFixed(1)} dB boost can
            distort on loud music. Lower the headroom slider to make room for it.
          </div>
        {/if}
      </section>
    {/if}

    <section class="card">
      <h2>Headroom</h2>
      <div class="slider-row">
        <input
          type="range"
          min="-12"
          max="0"
          step="0.5"
          value={headroomShown}
          oninput={onHeadroomInput}
          disabled={!status.eapo.installed}
        />
        <span class="slider-value">{headroomShown.toFixed(1)} dB</span>
      </div>
      <div class="hint">
        Extra margin for hot masters and intersample peaks. −3 dB is a comfortable default.
      </div>
    </section>

    {#if status.devices.some((d) => d.assignment && d.guid !== selectedDevice?.guid) || status.offline_assignments.length}
      <section class="card">
        <h2>Other active profiles</h2>
        <ul class="assigned">
          {#each status.devices.filter((d) => d.assignment && d.guid !== selectedDevice?.guid) as d (d.guid)}
            <li>
              <div>
                <span class="profile-name">{d.assignment?.name}</span>
                <span class="dim">on {d.name}</span>
              </div>
              <button class="remove" disabled={busy} onclick={() => onClear(d.guid)}>Remove</button>
            </li>
          {/each}
          {#each status.offline_assignments as o (o.device_guid)}
            <li class="offline">
              <div>
                <span class="profile-name">{o.assignment.name}</span>
                <span class="dim">on {o.device_name} (disconnected)</span>
              </div>
              <button class="remove" disabled={busy} onclick={() => onClear(o.device_guid)}>Remove</button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if actionError}
      <div class="banner error">{actionError}</div>
    {/if}

    <footer>
      <label class="autostart">
        <input type="checkbox" checked={status.autostart} onchange={onToggleAutostart} />
        Start with Windows (minimized to tray)
      </label>
      <div class="footer-right">
        {#if indexAge}
          <span class="dim">Database: {new Date(indexAge * 1000).toLocaleDateString()}</span>
        {/if}
        <button class="link" disabled={indexLoading} onclick={() => loadIndex(true)}>Refresh</button>
        <span class="dot">·</span>
        <button class="link" onclick={() => openUrl("https://github.com/jaakkopasanen/AutoEq")}>
          AutoEq
        </button>
      </div>
    </footer>
  {:else if !loadError}
    <div class="loading">Loading…</div>
  {/if}
</main>

<style>
  :global(:root) {
    color-scheme: light dark;
    --accent: #2f6fed;
    --bg: #f2f3f7;
    --text: #1c1f27;
    --card: #ffffff;
    --border: #dfe2ea;
    --dim: #5d6373;
    --faint: #868c9a;
    --surface: #f5f6fa;
    --surface-hover: #ecedf3;
    --elev: #f7f8fb;
    --muted-border: #d3d7e0;
    --knob-off: #b9bfcc;
    --knob: #ffffff;
    --link: #2456c4;
    --profile: #2456c4;
    --chip-bg: #e8eaf1;
    --chip-text: #4d5364;
    --ok-bg: #d8f1e1;
    --ok-text: #157347;
    --bad-bg: #f8dfe2;
    --bad-text: #b23a4b;
    --warn-bg: #fdf5da;
    --warn-border: #d9c26a;
    --err-bg: #fae3e5;
    --err-border: #d98a92;
    --upd-bg: #e5edfd;
    --danger-border: #e3b6bc;
    --danger-text: #b23a4b;
    --curve-grid: rgba(90, 98, 118, 0.18);
    --curve-zero: #b7bcc8;
    --curve-tick: #868c9a;
    --curve-raw: #6d7382;
    --curve-target: #58a6e8;
    --curve-shelf: #c97a26;
    --curve-total: #159a5c;
    --curve-corr: #2f6fed;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #0d0f13;
      --text: #e6e8ee;
      --card: #14171e;
      --border: #222735;
      --dim: #8b90a0;
      --faint: #6d7382;
      --surface: #0f1218;
      --surface-hover: #1a1e28;
      --elev: #171a22;
      --muted-border: #2a2e3a;
      --knob-off: #3a3f4e;
      --knob: #e6e8ee;
      --link: #6ea2ff;
      --profile: #9fc1ff;
      --chip-bg: #232734;
      --chip-text: #aab0c0;
      --ok-bg: #12301c;
      --ok-text: #6fd08c;
      --bad-bg: #331a1d;
      --bad-text: #e08a91;
      --warn-bg: #241f11;
      --warn-border: #5d4d1a;
      --err-bg: #241214;
      --err-border: #63272c;
      --upd-bg: #122036;
      --danger-border: #3a2a2d;
      --danger-text: #e08a91;
      --curve-grid: rgba(35, 39, 52, 0.4);
      --curve-zero: #3a3f4e;
      --curve-tick: #6d7382;
      --curve-raw: #8b90a0;
      --curve-target: #58a6e8;
      --curve-shelf: #f0a35e;
      --curve-total: #7ee0a3;
      --curve-corr: #6ea2ff;
    }
  }
  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif;
    font-size: 14px;
  }
  main {
    max-width: 680px;
    margin: 0 auto;
    padding: 22px 24px 30px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 2px 6px;
  }
  h1 {
    font-size: 21px;
    margin: 0;
    letter-spacing: 0.2px;
  }
  .subtitle {
    color: var(--dim);
    font-size: 12px;
  }
  .title {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .card {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h2 {
    font-size: 11.5px;
    text-transform: uppercase;
    letter-spacing: 0.9px;
    color: var(--dim);
    margin: 0;
  }
  .for-device {
    text-transform: none;
    letter-spacing: normal;
    color: var(--faint);
    font-weight: 400;
  }
  .dim {
    color: var(--dim);
    font-size: 12px;
  }

  .master-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--surface);
    border: 1px solid var(--muted-border);
    border-radius: 999px;
    padding: 6px 14px 6px 6px;
    color: var(--dim);
    cursor: pointer;
    transition: all 0.15s ease;
    font-size: 14px;
  }
  .master-toggle .knob {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--knob-off);
    transition: background 0.15s ease;
  }
  .master-toggle.on {
    border-color: var(--accent);
    color: var(--text);
    box-shadow: 0 0 14px rgba(47, 111, 237, 0.2);
  }
  .master-toggle.on .knob {
    background: var(--accent);
  }
  .master-toggle:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .banner {
    border-radius: 12px;
    padding: 12px 14px;
    line-height: 1.45;
  }
  .banner.warn {
    background: var(--warn-bg);
    border: 1px solid var(--warn-border);
  }
  .banner.error {
    background: var(--err-bg);
    border: 1px solid var(--err-border);
  }
  .banner.update {
    background: var(--upd-bg);
    border: 1px solid rgba(47, 111, 237, 0.4);
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .banner.update button {
    margin-left: auto;
  }
  .banner p {
    margin: 6px 0 10px;
    color: var(--text);
    opacity: 0.85;
  }
  .banner button {
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 8px;
    padding: 7px 14px;
    cursor: pointer;
  }

  .device-select {
    width: 100%;
    box-sizing: border-box;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: inherit;
    font-size: 14px;
    font-weight: 600;
    padding: 10px 12px;
    outline: none;
    cursor: pointer;
  }
  .device-select:focus {
    border-color: var(--accent);
  }
  .device-meta {
    display: flex;
    gap: 8px;
    color: var(--dim);
    font-size: 12px;
    align-items: center;
    flex-wrap: wrap;
  }
  .device-profile-inline {
    display: flex;
    gap: 6px;
    align-items: baseline;
  }
  .chip {
    font-size: 11px;
    border-radius: 999px;
    padding: 1px 8px;
    background: var(--chip-bg);
    color: var(--chip-text);
  }
  .chip.ok {
    background: var(--ok-bg);
    color: var(--ok-text);
  }
  .chip.bad {
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .profile-name {
    font-weight: 600;
    color: var(--profile);
    font-size: 13px;
  }

  input[type="search"] {
    width: 100%;
    box-sizing: border-box;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: inherit;
    font-size: 15px;
    padding: 11px 14px;
    outline: none;
  }
  input[type="search"]:focus {
    border-color: var(--accent);
  }

  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 300px;
    overflow-y: auto;
  }
  .result-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 7px 10px;
    border-radius: 8px;
  }
  .result-row:hover {
    background: var(--surface-hover);
  }
  .result-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .result-name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .result-sub {
    color: var(--dim);
    font-size: 12px;
  }
  .apply {
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 8px;
    padding: 6px 16px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .apply:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .apply.small {
    padding: 3px 12px;
    font-size: 12px;
  }
  .variants {
    list-style: none;
    margin: 0;
    padding: 0 10px 8px 22px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
    color: var(--chip-text);
  }
  .variants li {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .response-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .legend {
    display: flex;
    gap: 16px;
    font-size: 12px;
    color: var(--dim);
    padding: 0 2px;
  }
  .legend-spacer {
    flex: 1;
  }
  .legend-item {
    display: flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    color: inherit;
    font-size: inherit;
    padding: 2px 4px;
    cursor: pointer;
    border-radius: 6px;
    transition: opacity 0.12s ease;
  }
  .legend-item:hover {
    background: var(--surface-hover);
  }
  .legend-item.off {
    opacity: 0.38;
  }
  .legend-item.off .swatch {
    background: var(--knob-off) !important;
  }
  .swatch {
    display: inline-block;
    width: 14px;
    height: 3px;
    border-radius: 2px;
  }
  .swatch.raw {
    background: var(--curve-raw);
  }
  .swatch.target {
    background: var(--curve-target);
    opacity: 0.55;
  }
  .swatch.shelf {
    background: var(--curve-shelf);
  }
  .swatch.total {
    background: var(--curve-total);
  }

  .sliders {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .slider-row {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .slider-row input[type="range"] {
    flex: 1;
    accent-color: var(--accent);
  }
  .slider-label {
    color: var(--dim);
    font-size: 12px;
    min-width: 62px;
  }
  .slider-value {
    font-variant-numeric: tabular-nums;
    min-width: 62px;
    text-align: right;
    color: var(--profile);
    font-weight: 600;
  }

  .switch-row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--text);
  }
  .switch-spacer {
    flex: 1;
  }
  .switch {
    position: relative;
    width: 38px;
    height: 20px;
    flex-shrink: 0;
    border-radius: 999px;
    border: none;
    background: var(--knob-off);
    cursor: pointer;
    transition: background 0.15s ease;
    padding: 0;
  }
  .switch.on {
    background: var(--accent);
  }
  .switch-knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--knob);
    transition: transform 0.15s ease;
  }
  .switch.on .switch-knob {
    transform: translateX(18px);
  }

  .assigned {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .assigned li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: var(--elev);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px 14px;
  }
  .assigned li.offline {
    opacity: 0.65;
  }
  .remove {
    background: transparent;
    border: 1px solid var(--danger-border);
    color: var(--danger-text);
    border-radius: 8px;
    padding: 5px 12px;
    cursor: pointer;
    flex-shrink: 0;
  }

  .hint {
    color: var(--dim);
    font-size: 13px;
    line-height: 1.45;
  }
  .link {
    background: none;
    border: none;
    color: var(--link);
    cursor: pointer;
    padding: 0;
    font-size: inherit;
  }
  .link:disabled {
    opacity: 0.5;
  }

  footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 2px 0;
    color: var(--dim);
    font-size: 13px;
  }
  .autostart {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  .footer-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    color: var(--faint);
  }
  .loading {
    color: var(--dim);
    text-align: center;
    padding: 48px 0;
  }

</style>

