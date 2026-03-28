import type { Track, ScanProgress } from '../lib/types';
import * as ipc from '../lib/ipc';
import { listen } from '@tauri-apps/api/event';

let tracks = $state<Track[]>([]);
let sortBy = $state<string>('title');
let sortOrder = $state<string>('asc');
let searchQuery = $state('');
let scanStatus = $state<'idle' | 'scanning'>('idle');
let scanProgress = $state<ScanProgress | null>(null);
let folders = $state<string[]>([]);

export function getTracks() { return tracks; }
export function getScanStatus() { return scanStatus; }
export function getScanProgress() { return scanProgress; }
export function getFolders() { return folders; }

export async function loadTracks() {
  try {
    tracks = await ipc.getTracks(sortBy, sortOrder, searchQuery || undefined);
  } catch (e) {
    console.error('Failed to load tracks:', e);
  }
}

export async function loadFolders() {
  try {
    folders = await ipc.getFolders();
  } catch (e) {
    console.error('Failed to load folders:', e);
  }
}

export function setSort(column: string) {
  if (sortBy === column) {
    sortOrder = sortOrder === 'asc' ? 'desc' : 'asc';
  } else {
    sortBy = column;
    sortOrder = 'asc';
  }
  loadTracks();
}

// Debounce search to avoid IPC call on every keystroke
let searchTimeout: ReturnType<typeof setTimeout> | null = null;

export function setSearch(query: string) {
  searchQuery = query;
  if (searchTimeout) clearTimeout(searchTimeout);
  searchTimeout = setTimeout(() => {
    searchTimeout = null;
    loadTracks();
  }, 200);
}

export async function startScan(scanFolders: string[]) {
  scanStatus = 'scanning';
  scanProgress = { current: 0, total: 0, phase: 'Starting scan...' };
  try {
    await ipc.scanLibrary(scanFolders);
  } catch (e) {
    console.error('Scan failed:', e);
  }
  // The scan runs in a background thread on Rust side.
  // Progress/completion is tracked via the scan-progress event listener.
}

// Guard against multiple listener registrations
let scanListenerInitialized = false;

export async function initScanListener() {
  if (scanListenerInitialized) return;
  scanListenerInitialized = true;
  await listen<ScanProgress>('scan-progress', (event) => {
    scanProgress = event.payload;
    if (event.payload.phase === 'complete') {
      scanStatus = 'idle';
      scanProgress = null;
      loadTracks();
    }
  });
}
