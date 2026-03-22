import { invoke } from '@tauri-apps/api/core';
import type { Track, Extension } from './types';

export async function getTracks(sortBy: string, sortOrder: string, search?: string): Promise<Track[]> {
  return invoke('get_tracks', { sortBy, sortOrder, search: search ?? null });
}

export async function getCoverPath(trackId: string): Promise<string | null> {
  return invoke('get_cover_path', { trackId });
}

export async function addFolder(path: string): Promise<void> {
  return invoke('add_folder', { path });
}

export async function getFolders(): Promise<string[]> {
  return invoke('get_folders');
}

export async function scanLibrary(folders: string[]): Promise<void> {
  return invoke('scan_library', { folders });
}

export async function getWaveform(trackId: string): Promise<number[] | null> {
  return invoke('get_waveform', { trackId });
}

export async function fetchCoverArt(trackId: string, artist: string, title: string): Promise<string | null> {
  return invoke('fetch_cover_art', { trackId, artist, title });
}

export async function listExtensions(): Promise<Extension[]> {
  return invoke('list_extensions');
}
export async function installExtension(path: string): Promise<string> {
  return invoke('install_extension', { path });
}
export async function uninstallExtension(id: string): Promise<void> {
  return invoke('uninstall_extension', { id });
}
export async function startExtension(id: string): Promise<void> {
  return invoke('start_extension', { id });
}
export async function stopExtension(id: string): Promise<void> {
  return invoke('stop_extension', { id });
}
export async function extensionSearch(id: string, query: string): Promise<any> {
  return invoke('extension_search', { id, query });
}
