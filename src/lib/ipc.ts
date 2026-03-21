import { invoke } from '@tauri-apps/api/core';
import type { Track } from './types';

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
