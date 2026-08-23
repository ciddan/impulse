<script lang="ts">
  interface Props {
    /** Correction curve (what the convolver realizes): [hz, dB][] */
    points: [number, number][];
    /** Measured response to display (raw or smoothed variant). */
    raw?: [number, number][];
    /** Target curve, optional. */
    target?: [number, number][];
    bassGain: number;
    bassFc: number;
    /** Per-series visibility; main = corrected/correction line. */
    show?: { raw: boolean; target: boolean; shelf: boolean; main: boolean };
  }
  let {
    points,
    raw = [],
    target = [],
    bassGain,
    bassFc,
    show = { raw: true, target: true, shelf: true, main: true },
  }: Props = $props();

  const FMIN = 20;
  const FMAX = 20000;
  const W = 660;
  const H = 250;
  const L = 36;
  const R = 12;
  const T = 12;
  const B = 26;
  const N = 180;

  /** RBJ low-shelf magnitude in dB (S=0.9, close to EAPO's LS slope). */
  function shelfDb(f: number, gain: number, fc: number): number {
    if (gain === 0) return 0;
    const fs = 48000;
    const A = Math.pow(10, gain / 40);
    const w0 = (2 * Math.PI * fc) / fs;
    const S = 0.9;
    const alpha = (Math.sin(w0) / 2) * Math.sqrt((A + 1 / A) * (1 / S - 1) + 2);
    const cw = Math.cos(w0);
    const sA = Math.sqrt(A);
    const b = [
      A * (A + 1 - (A - 1) * cw + 2 * sA * alpha),
      2 * A * (A - 1 - (A + 1) * cw),
      A * (A + 1 - (A - 1) * cw - 2 * sA * alpha),
    ];
    const a = [
      A + 1 + (A - 1) * cw + 2 * sA * alpha,
      -2 * (A - 1 + (A + 1) * cw),
      A + 1 + (A - 1) * cw - 2 * sA * alpha,
    ];
    const w = (2 * Math.PI * f) / fs;
    const mag = (c: number[]) =>
      Math.hypot(
        c[0] + c[1] * Math.cos(w) + c[2] * Math.cos(2 * w),
        -c[1] * Math.sin(w) - c[2] * Math.sin(2 * w),
      );
    return 20 * Math.log10(mag(b) / mag(a));
  }

  /** Linear interpolation in log-frequency space; NaN outside data range. */
  function interp(pts: [number, number][], f: number, clampEnds = true): number {
    if (pts.length === 0) return NaN;
    if (f <= pts[0][0]) return clampEnds ? pts[0][1] : NaN;
    const last = pts[pts.length - 1];
    if (f >= last[0]) return clampEnds ? last[1] : NaN;
    let lo = 0;
    let hi = pts.length - 1;
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1;
      if (pts[mid][0] <= f) lo = mid;
      else hi = mid;
    }
    const [f0, g0] = pts[lo];
    const [f1, g1] = pts[hi];
    const t = (Math.log10(f) - Math.log10(f0)) / (Math.log10(f1) - Math.log10(f0));
    return g0 + t * (g1 - g0);
  }

  const samples = $derived.by(() => {
    const out: {
      f: number;
      raw: number;
      target: number;
      corr: number;
      shelf: number;
      total: number;
      corrected: number;
    }[] = [];
    for (let i = 0; i <= N; i++) {
      const f = FMIN * Math.pow(FMAX / FMIN, i / N);
      const r = interp(raw, f, false);
      const corr = interp(points, f);
      const shelf = shelfDb(f, bassGain, bassFc);
      out.push({
        f,
        raw: r,
        target: interp(target, f, false),
        corr,
        shelf,
        total: corr + shelf,
        // Post-EQ response: measured response + the realized correction
        // (itself measured from the convolver IR) + shelf.
        corrected: r + corr + shelf,
      });
    }
    return out;
  });

  const hasRaw = $derived(raw.length > 0);
  const hasTarget = $derived(target.length > 0);

  const yMax = $derived.by(() => {
    let m = 3;
    for (const s of samples) {
      const vals: number[] = [];
      if (hasRaw && show.raw) vals.push(s.raw);
      if (show.target) vals.push(s.target);
      if (show.shelf) vals.push(s.shelf);
      if (show.main) vals.push(hasRaw ? s.corrected : s.total);
      for (const v of vals) {
        if (Number.isFinite(v)) m = Math.max(m, Math.abs(v));
      }
    }
    return Math.ceil((m + 1) / 3) * 3;
  });

  const x = (f: number) =>
    L + ((Math.log10(f) - Math.log10(FMIN)) / (Math.log10(FMAX) - Math.log10(FMIN))) * (W - L - R);
  const y = (db: number) => T + ((yMax - db) / (2 * yMax)) * (H - T - B);

  type Key = "raw" | "target" | "corr" | "shelf" | "total" | "corrected";
  function path(key: Key): string {
    let d = "";
    let pen = false;
    for (const s of samples) {
      const v = s[key];
      if (!Number.isFinite(v)) {
        pen = false;
        continue;
      }
      d += `${pen ? "L" : "M"}${x(s.f).toFixed(1)} ${y(v).toFixed(1)}`;
      pen = true;
    }
    return d;
  }

  const fGrid = [50, 100, 200, 500, 1000, 2000, 5000, 10000];
  const fLabels: [number, string][] = [
    [20, "20"],
    [100, "100"],
    [1000, "1k"],
    [10000, "10k"],
    [20000, "20k"],
  ];
  const dbGrid = $derived.by(() => {
    const step = yMax > 12 ? 6 : 3;
    const lines: number[] = [];
    for (let v = -yMax; v <= yMax; v += step) lines.push(v);
    return lines;
  });
</script>

<svg viewBox="0 0 {W} {H}" class="curve" role="img" aria-label="Frequency response">
  {#each fGrid as f (f)}
    <line x1={x(f)} y1={T} x2={x(f)} y2={H - B} class="grid" />
  {/each}
  {#each dbGrid as db (db)}
    <line x1={L} y1={y(db)} x2={W - R} y2={y(db)} class="grid" class:zero={db === 0} />
    <text x={L - 5} y={y(db) + 3} class="tick" text-anchor="end">{db > 0 ? "+" : ""}{db}</text>
  {/each}
  {#each fLabels as [f, label] (f)}
    <text x={x(f)} y={H - B + 15} class="tick" text-anchor="middle">{label}</text>
  {/each}
  {#if hasTarget && show.target}
    <path d={path("target")} class="line target" />
  {/if}
  {#if hasRaw && show.raw}
    <path d={path("raw")} class="line raw" />
  {/if}
  {#if bassGain !== 0 && show.shelf}
    <path d={path("shelf")} class="line shelf" />
  {/if}
  <!-- With measurement data, show the post-EQ response (raw + correction +
       shelf); without it, fall back to the correction curve itself. -->
  {#if show.main}
    <path d={path(hasRaw ? "corrected" : "total")} class="line total" />
  {/if}
</svg>

<style>
  .curve {
    width: 100%;
    height: auto;
    display: block;
  }
  .grid {
    stroke: var(--curve-grid);
    stroke-width: 1;
  }
  .grid.zero {
    stroke: var(--curve-zero);
  }
  .tick {
    fill: var(--curve-tick);
    font-size: 9px;
    font-family: inherit;
  }
  .line {
    fill: none;
    stroke-width: 1.5;
    stroke-linejoin: round;
  }
  .line.raw {
    stroke: var(--curve-raw);
    stroke-width: 1.25;
    opacity: 0.8;
  }
  .line.target {
    stroke: var(--curve-target);
    stroke-width: 4;
    opacity: 0.35;
    stroke-linecap: round;
  }
  .line.shelf {
    stroke: var(--curve-shelf);
    opacity: 0.7;
  }
  .line.total {
    stroke: var(--curve-total);
    stroke-width: 2.25;
  }
</style>
