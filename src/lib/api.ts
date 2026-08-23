import { invoke } from "@tauri-apps/api/core";

export interface EapoStatus {
	installed: boolean;
	config_dir: string | null;
	install_dir: string | null;
	include_installed: boolean;
}

export interface AssignedProfile {
	source: string;
	form: string;
	name: string;
	has_ir: boolean;
	bass_gain_db: number;
	bass_fc: number;
}

export interface DeviceView {
	id: string;
	guid: string;
	name: string;
	sample_rate: number;
	is_default: boolean;
	apo_enabled: boolean;
	assignment: AssignedProfile | null;
}

export interface OfflineAssignment {
	device_guid: string;
	device_name: string;
	assignment: AssignedProfile;
}

export interface AppStatus {
	eapo: EapoStatus;
	devices: DeviceView[];
	offline_assignments: OfflineAssignment[];
	master_enabled: boolean;
	headroom_db: number;
	compensate_shelf: boolean;
	autostart: boolean;
	mica: boolean;
	accent: string | null;
	accent_light: string | null;
	accent_dark: string | null;
}

export interface HeadphoneEntry {
	source: string;
	form: string;
	name: string;
	rank: number;
	has_ir: boolean;
}

export interface IndexCache {
	fetched_at: number;
	entries: HeadphoneEntry[];
}

export const getStatus = () => invoke<AppStatus>("get_status");
export const getIndex = (force = false) => invoke<IndexCache>("get_index", { force });
export const applyProfile = (deviceGuid: string, deviceName: string, entry: HeadphoneEntry) =>
	invoke<AppStatus>("apply_profile", {
		deviceGuid,
		deviceName,
		source: entry.source,
		form: entry.form,
		name: entry.name,
		hasIr: entry.has_ir,
	});
export const applyCustomIr = (deviceGuid: string, deviceName: string) =>
	invoke<AppStatus | null>("apply_custom_ir", { deviceGuid, deviceName });
export const clearProfile = (deviceGuid: string) =>
	invoke<AppStatus>("clear_profile", { deviceGuid });
export const setMaster = (enabled: boolean) => invoke<AppStatus>("set_master", { enabled });
export interface CurveData {
	eq: [number, number][];
	eq_source: "convolver" | "graphiceq" | string;
	ir_preamp_db: number;
	raw: [number, number][];
	smoothed: [number, number][];
	equalized_raw: [number, number][];
	equalized_smoothed: [number, number][];
	target: [number, number][];
}

export const getCurve = (deviceGuid: string) => invoke<CurveData>("get_curve", { deviceGuid });
export const setBassShelf = (deviceGuid: string, gainDb: number, fc: number) =>
	invoke<AppStatus>("set_bass_shelf", { deviceGuid, gainDb, fc });
export const setHeadroom = (db: number) => invoke<AppStatus>("set_headroom", { db });
export const setCompensateShelf = (enabled: boolean) =>
	invoke<AppStatus>("set_compensate_shelf", { enabled });
export const setAutostart = (enabled: boolean) => invoke<boolean>("set_autostart", { enabled });
export const setupEapoInclude = () => invoke<AppStatus>("setup_eapo_include");
export const openConfigurator = () => invoke<void>("open_configurator");

/** Group index entries by headphone name; sources sorted best-first. */
export interface GroupedHeadphone {
	name: string;
	variants: HeadphoneEntry[]; // sorted by rank
}

export function groupIndex(entries: HeadphoneEntry[]): GroupedHeadphone[] {
	const map = new Map<string, HeadphoneEntry[]>();
	for (const e of entries) {
		const list = map.get(e.name);
		if (list) list.push(e);
		else map.set(e.name, [e]);
	}
	const groups: GroupedHeadphone[] = [];
	for (const [name, variants] of map) {
		variants.sort((a, b) => a.rank - b.rank || a.source.localeCompare(b.source));
		groups.push({ name, variants });
	}
	groups.sort((a, b) => a.name.localeCompare(b.name));
	return groups;
}

/** Simple ranked search: prefix > word-prefix > substring > subsequence. */
export function searchGroups(
	groups: GroupedHeadphone[],
	query: string,
	limit = 30,
): GroupedHeadphone[] {
	const q = query.trim().toLowerCase();
	if (!q) return [];
	const scored: { g: GroupedHeadphone; score: number }[] = [];
	for (const g of groups) {
		const n = g.name.toLowerCase();
		let score = -1;
		if (n.startsWith(q)) score = 0;
		else if (n.split(/\s+/).some((w) => w.startsWith(q))) score = 1;
		else if (n.includes(q)) score = 2;
		else {
			// All query tokens must appear somewhere.
			const tokens = q.split(/\s+/).filter(Boolean);
			if (tokens.length > 1 && tokens.every((t) => n.includes(t))) score = 3;
		}
		if (score >= 0) scored.push({ g, score: score * 1000 + g.variants[0].rank });
	}
	scored.sort((a, b) => a.score - b.score || a.g.name.localeCompare(b.g.name));
	return scored.slice(0, limit).map((s) => s.g);
}
