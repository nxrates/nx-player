import type { Track } from '../lib/types';
import { audioPlayer } from '../lib/audio';
import { getCoverPath, getWaveform, fetchCoverArt } from '../lib/ipc';
import { convertFileSrc } from '@tauri-apps/api/core';

type RepeatMode = 'off' | 'all' | 'one';

let status = $state<'stopped' | 'playing' | 'paused'>('stopped');
let currentTrack = $state<Track | null>(null);
let queue = $state<Track[]>([]);
let queueIndex = $state(-1);
let shuffle = $state(localStorage.getItem('ls-shuffle') === 'true');
let repeat = $state<RepeatMode>((localStorage.getItem('ls-repeat') as RepeatMode) || 'off');
let volume = $state(parseFloat(localStorage.getItem('ls-volume') ?? '0.8'));
let muted = $state(false);
let position = $state(0);
let duration = $state(0);
let playbackRate = $state(parseFloat(localStorage.getItem('ls-playback-rate') ?? '1'));
let coverUrl = $state<string | null>(null);
let playHistory = $state<number[]>([]);
let isCrossfading = false;

// Crossfade visualization state
let crossfadeProgress = $state(0);
let outgoingTrack = $state<Track | null>(null);
let outgoingCoverUrl = $state<string | null>(null);
let outgoingPosition = $state(0);
let outgoingDuration = $state(0);
let crossfadeIntervalId: ReturnType<typeof setInterval> | null = null;

// Apply initial volume
audioPlayer.setVolume(volume);
audioPlayer.setPlaybackRate(playbackRate);

// Wire callbacks
audioPlayer.onTimeUpdate = (time) => {
  if (isCrossfading && outgoingTrack) {
    // During crossfade, active deck is the outgoing track
    outgoingPosition = time;
    checkAutoMix(time);
  } else {
    position = time;
    checkAutoMix(time);
  }
};
audioPlayer.onLoadedMetadata = (dur) => {
  if (!isCrossfading) {
    duration = dur;
  }
};
audioPlayer.onEnded = () => { if (!isCrossfading) next(); };
audioPlayer.onError = (err) => {
  console.error('Audio error:', err);
  status = 'stopped';
};

audioPlayer.onCrossfadeComplete = () => {
  isCrossfading = false;
  outgoingTrack = null;
  crossfadeProgress = 0;
};

audioPlayer.setMediaSessionHandlers({
  onPrev: () => prev(),
  onNext: () => next(),
});

export function getStatus() { return status; }
export function getCurrentTrack() { return currentTrack; }
export function getQueue() { return queue; }
export function getShuffle() { return shuffle; }
export function getRepeat() { return repeat; }
export function getVolume() { return volume; }
export function getMuted() { return muted; }
export function getPosition() { return position; }
export function getDuration() { return duration; }
export function getPlaybackRate() { return playbackRate; }
export function getCoverUrl() { return coverUrl; }
export function getCrossfadeProgress() { return crossfadeProgress; }
export function getOutgoingTrack() { return outgoingTrack; }
export function getOutgoingPosition() { return outgoingPosition; }
export function getOutgoingDuration() { return outgoingDuration; }

export async function playTrack(track: Track, trackList?: Track[]) {
  if (trackList) {
    queue = [...trackList];
    queueIndex = trackList.findIndex(t => t.id === track.id);
    if (queueIndex === -1) queueIndex = 0;
  }
  currentTrack = track;
  status = 'playing';
  position = 0;

  // Load cover art
  coverUrl = null;
  if (track.has_cover) {
    try {
      const path = await getCoverPath(track.id);
      if (path) coverUrl = convertFileSrc(path);
    } catch { /* no cover */ }
  }

  // Fallback: fetch from iTunes if no embedded cover
  if (!coverUrl && track.artist && track.title) {
    try {
      const fetchedPath = await fetchCoverArt(track.id, track.artist, track.title);
      if (fetchedPath) coverUrl = convertFileSrc(fetchedPath);
    } catch { /* no cover found online either */ }
  }

  try {
    const src = convertFileSrc(track.path);
    await audioPlayer.play(src, {
      title: track.title,
      artist: track.artist,
      album: track.album,
      artwork: coverUrl ?? undefined,
    });
  } catch (e) {
    console.error('Failed to play track:', e);
    status = 'stopped';
  }

  // Pre-process next tracks for BPM/waveform
  preProcessNextTracks();
}

async function preProcessNextTracks() {
  for (let offset = 1; offset <= 2; offset++) {
    const idx = queueIndex + offset;
    if (idx < queue.length) {
      const t = queue[idx];
      if (t && !t.bpm) {
        try { await getWaveform(t.id); } catch {}
      }
    }
  }
}

export function playPause() {
  // If no track is loaded, pick a random one from the queue and play it
  if (!currentTrack) {
    if (queue.length > 0) {
      const idx = Math.floor(Math.random() * queue.length);
      queueIndex = idx;
      playTrack(queue[idx]);
    }
    return;
  }
  if (status === 'stopped') {
    // Track already selected (e.g., from startup auto-select), just play it
    playTrack(currentTrack);
    return;
  }
  if (status === 'playing') {
    if (isCrossfading) {
      audioPlayer.pauseAll();
    } else {
      audioPlayer.pause();
    }
    status = 'paused';
  } else {
    if (isCrossfading) {
      audioPlayer.resumeAll();
    } else {
      audioPlayer.resume();
    }
    status = 'playing';
  }
}

// Select a random track and set it as current WITHOUT playing
export function selectRandom(trackList?: Track[]) {
  if (trackList && trackList.length > 0) {
    queue = [...trackList];
  }
  if (queue.length === 0) return;
  const idx = Math.floor(Math.random() * queue.length);
  queueIndex = idx;
  currentTrack = queue[idx];
  // Load cover
  if (queue[idx].has_cover) {
    getCoverPath(queue[idx].id).then(path => {
      if (path) coverUrl = convertFileSrc(path);
    }).catch(() => {});
  } else {
    coverUrl = null;
  }
}

// --- Auto-mix ---

function checkAutoMix(currentTime: number) {
  if (isCrossfading || status !== 'playing' || duration <= 0) return;

  const automixOn = localStorage.getItem('ls-automix') === 'true';
  const crossfadeOn = localStorage.getItem('ls-crossfade') === 'true';
  if (!automixOn || !crossfadeOn) return;

  const crossfadeDur = parseFloat(localStorage.getItem('ls-crossfade-dur') ?? '8');
  const mixPoint = duration - crossfadeDur;

  // Don't trigger for very short tracks
  if (mixPoint <= 0 || duration < crossfadeDur * 2) return;

  if (currentTime >= mixPoint) {
    startAutoMix(crossfadeDur);
  }
}

async function startAutoMix(crossfadeDur: number) {
  if (isCrossfading) return;
  isCrossfading = true;

  // Push current index to history before advancing
  if (queueIndex >= 0) {
    playHistory = [...playHistory, queueIndex];
  }

  // Determine next track index
  let nextIndex = queueIndex + 1;
  if (shuffle) {
    nextIndex = Math.floor(Math.random() * queue.length);
  }
  if (nextIndex >= queue.length) {
    if (repeat === 'all') {
      nextIndex = 0;
    } else {
      isCrossfading = false;
      return;
    }
  }
  if (repeat === 'one') {
    nextIndex = queueIndex;
  }

  const nextTrack = queue[nextIndex];
  if (!nextTrack) {
    isCrossfading = false;
    return;
  }

  // BPM matching: adjust next track's speed to match current track's BPM
  const matchBpm = localStorage.getItem('ls-match-bpm') === 'true';
  if (matchBpm && currentTrack?.bpm && nextTrack.bpm) {
    const ratio = currentTrack.bpm / nextTrack.bpm; // slow down if next is faster, speed up if slower
    if (ratio >= 0.85 && ratio <= 1.15) {
      audioPlayer.setIncomingPlaybackRate(playbackRate * ratio);
    }
  }

  // Load cover for next track
  let nextCoverUrl: string | null = null;
  if (nextTrack.has_cover) {
    try {
      const path = await getCoverPath(nextTrack.id);
      if (path) nextCoverUrl = convertFileSrc(path);
    } catch { /* no cover */ }
  }

  // Save outgoing track info before swapping
  outgoingTrack = currentTrack;
  outgoingCoverUrl = coverUrl;
  outgoingPosition = position;
  outgoingDuration = duration;
  crossfadeProgress = 0;

  const nextSrc = convertFileSrc(nextTrack.path);
  try {
    await audioPlayer.startCrossfade(nextSrc, crossfadeDur * 1000, {
      title: nextTrack.title,
      artist: nextTrack.artist,
      album: nextTrack.album,
      artwork: nextCoverUrl ?? undefined,
    });

    // Update player state to the new track
    queueIndex = nextIndex;
    currentTrack = nextTrack;
    coverUrl = nextCoverUrl;
    position = 0;

    // Reset playback rate on the now-active deck if we adjusted it
    if (matchBpm) {
      audioPlayer.setPlaybackRate(playbackRate);
    }
  } catch (e) {
    console.error('Auto-mix crossfade failed:', e);
    isCrossfading = false;
    outgoingTrack = null;
    outgoingCoverUrl = null;
    crossfadeProgress = 0;
    return;
  }

  // Animate crossfade progress and track positions
  const startTime = Date.now();
  if (crossfadeIntervalId) clearInterval(crossfadeIntervalId);
  crossfadeIntervalId = setInterval(() => {
    const elapsed = (Date.now() - startTime) / (crossfadeDur * 1000);
    crossfadeProgress = Math.min(1, elapsed);
    // Update incoming (current) track position from the audio player
    if (audioPlayer.isCrossfading) {
      position = audioPlayer.incomingTime;
      const inDur = audioPlayer.incomingDuration;
      if (inDur && !isNaN(inDur)) duration = inDur;
    }
    if (crossfadeProgress >= 1) {
      clearInterval(crossfadeIntervalId!);
      crossfadeIntervalId = null;
      outgoingTrack = null;
      outgoingCoverUrl = null;
      crossfadeProgress = 0;
    }
  }, 250);

}

export function next() {
  if (queue.length === 0) return;
  if (repeat === 'one') {
    const track = queue[queueIndex];
    if (track) playTrack(track);
    return;
  }
  // Push current index to history before advancing
  if (queueIndex >= 0) {
    playHistory = [...playHistory, queueIndex];
  }
  let nextIndex = queueIndex + 1;
  if (shuffle) {
    nextIndex = Math.floor(Math.random() * queue.length);
  }
  if (nextIndex >= queue.length) {
    if (repeat === 'all') {
      nextIndex = 0;
    } else {
      status = 'stopped';
      return;
    }
  }
  queueIndex = nextIndex;
  playTrack(queue[nextIndex]);
}

export function prev() {
  if (queue.length === 0) return;

  // If crossfading, cancel and restore outgoing track
  if (isCrossfading && outgoingTrack) {
    audioPlayer.cancelCrossfade();
    currentTrack = outgoingTrack;
    coverUrl = outgoingCoverUrl;
    position = outgoingPosition;
    duration = outgoingDuration;
    outgoingTrack = null;
    outgoingCoverUrl = null;
    crossfadeProgress = 0;
    isCrossfading = false;
    if (crossfadeIntervalId) {
      clearInterval(crossfadeIntervalId);
      crossfadeIntervalId = null;
    }
    status = 'playing';
    return;
  }

  if (position > 3) {
    audioPlayer.seek(0);
    position = 0;
    return;
  }

  // If we have shuffle history, go back to the last track
  if (playHistory.length > 0) {
    const prevIdx = playHistory[playHistory.length - 1];
    playHistory = playHistory.slice(0, -1);
    queueIndex = prevIdx;
    playTrack(queue[prevIdx]);
    return;
  }

  let prevIndex = queueIndex - 1;
  if (prevIndex < 0) {
    if (repeat === 'all') {
      prevIndex = queue.length - 1;
    } else {
      prevIndex = 0;
    }
  }
  queueIndex = prevIndex;
  playTrack(queue[prevIndex]);
}

export function seek(time: number) {
  audioPlayer.seek(time);
  position = time;
}

export function setVolume(v: number) {
  volume = Math.max(0, Math.min(2, v));
  muted = false;
  audioPlayer.setVolume(volume);
  localStorage.setItem('ls-volume', String(volume));
}

export function toggleMute() {
  muted = !muted;
  audioPlayer.setVolume(muted ? 0 : volume);
}

export function toggleShuffle() {
  shuffle = !shuffle;
  localStorage.setItem('ls-shuffle', String(shuffle));
}

export function cycleRepeat() {
  if (repeat === 'off') repeat = 'all';
  else if (repeat === 'all') repeat = 'one';
  else repeat = 'off';
  localStorage.setItem('ls-repeat', repeat);
}

export function setPlaybackRate(r: number) {
  playbackRate = r;
  audioPlayer.setPlaybackRate(r);
  localStorage.setItem('ls-playback-rate', String(r));
}

export function removeFromQueue(index: number) {
  queue = queue.filter((_, i) => i !== index);
  if (index < queueIndex) queueIndex--;
  if (index === queueIndex && queueIndex >= queue.length) {
    queueIndex = Math.max(0, queue.length - 1);
  }
}

export function clearQueue() {
  queue = [];
  queueIndex = -1;
}
