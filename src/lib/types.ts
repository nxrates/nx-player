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
  file_size: number;
  mtime: number;
}

export interface ScanProgress {
  current: number;
  total: number;
  phase: string;
}
