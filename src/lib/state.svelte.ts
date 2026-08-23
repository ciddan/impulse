import { listen } from "@tauri-apps/api/event";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
	getStatus,
	getIndex,
	getCurve,
	applyProfile,
	applyCustomIr,
	clearProfile,
	setMaster,
	setBassShelf,
	setHeadroom,
	setCompensateShelf,
	setAutostart,
	setupEapoInclude,
	groupIndex,
	searchGroups,
	type AppStatus,
	type CurveData,
	type DeviceView,
	type GroupedHeadphone,
	type HeadphoneEntry,
} from "$lib/api";

/** Convolution only runs when an IR matches the device's mix rate. */
export function convolutionActive(d: DeviceView): boolean {
	return !!d.assignment?.has_ir && (d.sample_rate === 44100 || d.sample_rate === 48000);
}

const SLIDER_DEBOUNCE_MS = 250;

class App {
	status = $state<AppStatus | null>(null);
	groups = $state<GroupedHeadphone[]>([]);
	indexAge = $state<number | null>(null);
	loadError = $state<string | null>(null);
	actionError = $state<string | null>(null);
	busy = $state(false);
	indexLoading = $state(false);

	selectedGuid = $state<string | null>(null);
	query = $state("");
	expandedResult = $state<string | null>(null);

	pendingUpdate = $state<Update | null>(null);
	updating = $state(false);

	curveData = $state<CurveData | null>(null);
	smoothedView = $state(true);
	seriesVisible = $state({ raw: true, target: true, shelf: true, main: true });

	headroomDraft = $state<number | null>(null);
	bassDraft = $state<{ gain: number; fc: number } | null>(null);

	#curveFor = "";
	#headroomTimer: ReturnType<typeof setTimeout> | undefined;
	#bassTimer: ReturnType<typeof setTimeout> | undefined;

	results = $derived(searchGroups(this.groups, this.query));

	selected = $derived(
		this.status?.devices.find((d) => d.guid === this.selectedGuid) ??
			this.status?.devices.find((d) => d.is_default) ??
			this.status?.devices[0] ??
			null,
	);

	headroomShown = $derived(this.headroomDraft ?? this.status?.headroom_db ?? -3);

	bassShown = $derived(
		this.bassDraft ?? {
			gain: this.selected?.assignment?.bass_gain_db ?? 0,
			fc: this.selected?.assignment?.bass_fc ?? 350,
		},
	);

	rawSeries = $derived.by(() => {
		if (!this.curveData) return [] as [number, number][];
		if (this.smoothedView && this.curveData.smoothed.length) return this.curveData.smoothed;
		return this.curveData.raw;
	});

	/** Start-up work; returns a cleanup for the status-changed listener. */
	init(): () => void {
		this.refreshStatus();
		this.loadIndex();
		this.checkForUpdate();
		const unlisten = listen("status-changed", () => this.refreshStatus());
		return () => {
			unlisten.then((fn) => fn());
		};
	}

	async refreshStatus() {
		try {
			this.status = await getStatus();
			this.loadError = null;
			if (!this.selectedGuid && this.status.devices.length) {
				this.selectedGuid = (
					this.status.devices.find((d) => d.is_default) ?? this.status.devices[0]
				).guid;
			}
		} catch (e) {
			this.loadError = String(e);
		}
	}

	async loadIndex(force = false) {
		this.indexLoading = true;
		try {
			const cache = await getIndex(force);
			this.groups = groupIndex(cache.entries);
			this.indexAge = cache.fetched_at;
			this.actionError = null;
		} catch (e) {
			this.actionError = `Could not load the AutoEq database: ${e}`;
		} finally {
			this.indexLoading = false;
		}
	}

	async checkForUpdate() {
		try {
			this.pendingUpdate = await check();
		} catch {
			// Offline or no releases yet — silently skip.
		}
	}

	async installUpdate() {
		if (!this.pendingUpdate) return;
		this.updating = true;
		try {
			await this.pendingUpdate.downloadAndInstall();
			await relaunch();
		} catch (e) {
			this.actionError = `Update failed: ${e}`;
			this.updating = false;
		}
	}

	/** Fetch the curve for the selected device when its profile changes. */
	loadCurve() {
		const assignment = this.selected?.assignment;
		const guid = this.selected?.guid;
		if (!assignment || !guid) {
			this.curveData = null;
			this.#curveFor = "";
			return;
		}
		const key = `${guid}|${assignment.source}|${assignment.name}`;
		if (key === this.#curveFor) return;
		this.#curveFor = key;
		getCurve(guid)
			.then((c) => (this.curveData = c))
			.catch(() => (this.curveData = null));
	}

	async apply(entry: HeadphoneEntry) {
		const device = this.selected;
		if (!device) return;
		this.busy = true;
		this.actionError = null;
		try {
			this.status = await applyProfile(device.guid, device.name, entry);
			this.query = "";
			this.expandedResult = null;
		} catch (e) {
			this.actionError = String(e);
		} finally {
			this.busy = false;
		}
	}

	async applyCustom() {
		const device = this.selected;
		if (!device) return;
		this.busy = true;
		this.actionError = null;
		try {
			const s = await applyCustomIr(device.guid, device.name);
			if (s) this.status = s;
		} catch (e) {
			this.actionError = String(e);
		} finally {
			this.busy = false;
		}
	}

	async clear(guid: string) {
		this.busy = true;
		this.actionError = null;
		try {
			this.status = await clearProfile(guid);
		} catch (e) {
			this.actionError = String(e);
		} finally {
			this.busy = false;
		}
	}

	async toggleMaster() {
		if (!this.status) return;
		this.busy = true;
		try {
			this.status = await setMaster(!this.status.master_enabled);
		} catch (e) {
			this.actionError = String(e);
		} finally {
			this.busy = false;
		}
	}

	async toggleAutostart() {
		if (!this.status) return;
		try {
			const enabled = await setAutostart(!this.status.autostart);
			this.status = { ...this.status, autostart: enabled };
		} catch (e) {
			this.actionError = String(e);
		}
	}

	async toggleCompensateShelf() {
		if (!this.status) return;
		try {
			this.status = await setCompensateShelf(!this.status.compensate_shelf);
		} catch (e) {
			this.actionError = String(e);
		}
	}

	async setupInclude() {
		this.busy = true;
		try {
			this.status = await setupEapoInclude();
		} catch (e) {
			this.actionError = String(e);
		} finally {
			this.busy = false;
		}
	}

	/** Debounced so dragging doesn't hammer Equalizer APO with reloads. */
	nudgeHeadroom(db: number) {
		this.headroomDraft = db;
		clearTimeout(this.#headroomTimer);
		this.#headroomTimer = setTimeout(async () => {
			try {
				this.status = await setHeadroom(db);
			} catch (e) {
				this.actionError = String(e);
			} finally {
				this.headroomDraft = null;
			}
		}, SLIDER_DEBOUNCE_MS);
	}

	nudgeBass(field: "gain" | "fc", value: number) {
		const device = this.selected;
		if (!device?.assignment) return;
		const next = { ...this.bassShown, [field]: value };
		this.bassDraft = next;
		const guid = device.guid;
		clearTimeout(this.#bassTimer);
		this.#bassTimer = setTimeout(async () => {
			try {
				this.status = await setBassShelf(guid, next.gain, next.fc);
			} catch (e) {
				this.actionError = String(e);
			} finally {
				this.bassDraft = null;
			}
		}, SLIDER_DEBOUNCE_MS);
	}
}

export const app = new App();
