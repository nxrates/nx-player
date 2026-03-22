export interface Track {
  id: string;
  path: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string;
  genre: string;
  year: number | null;
  track_no: number | null;
  disc_no: number | null;
  duration: number;
  has_cover: boolean;
  bpm: number | null;
  /** Beat grid: timestamps in seconds for every detected beat */
  beat_grid: number[] | null;
  /** Downbeat (bar start) timestamps in seconds */
  downbeats: number[] | null;
  /** Musical key in Camelot notation (e.g., "8A", "11B") */
  key: string | null;
  file_size: number;
  mtime: number;
  source?: string;
}

export interface Extension {
  id: string;
  name: string;
  description: string;
  version: string;
  running: boolean;
  settings: ExtensionSetting[];
}

export interface ExtensionSetting {
  key: string;
  type: string;
  label: string;
  default?: string;
}

export interface ScanProgress {
  current: number;
  total: number;
  phase: string;
}
