/** Duration shared by the original score and the visual composition, in seconds. */
export const CLOUD_SCORE_DURATION = 20;

/**
 * Optional soundtrack for the cloud composition. Construction is SSR-safe.
 *
 * The owner supplies visual timeline seconds. Call play(time) when playback
 * starts, seek(time) after scrubbing, and pause() when paused/hidden. Sound is
 * initially muted. In an explicit Sound on click handler, call setMuted(false)
 * followed by play(currentVisualTime); a false result means playback was denied
 * or unavailable and the UI should return to its muted state.
 *
 * No audio is fetched or decoded during silent autoplay. This class does not
 * create its own timer or loop: the visual timeline remains authoritative.
 */
export class CloudScore {
  private audio: HTMLAudioElement | null = null;
  private muted = true;
  private targetTime = 0;
  private wantsPlayback = false;
  private generation = 0;
  private disposed = false;

  constructor(private readonly source = '/media/cloud/reveal-score.m4a') {}

  private ensureAudio(): HTMLAudioElement | null {
    if (this.disposed || typeof Audio === 'undefined') return null;
    if (!this.audio) {
      const audio = new Audio(this.source);
      audio.preload = 'none';
      audio.muted = this.muted;
      audio.volume = 0.7;
      audio.addEventListener('loadedmetadata', this.applyTime);
      this.audio = audio;
    }
    return this.audio;
  }

  private applyTime = (): void => {
    const audio = this.audio;
    if (!audio) return;
    const duration = Number.isFinite(audio.duration) ? audio.duration : CLOUD_SCORE_DURATION;
    try {
      audio.currentTime = Math.min(this.targetTime, duration);
    } catch {
      // Some engines require metadata before accepting an initial seek.
      // loadedmetadata reapplies the most recent requested timeline position.
    }
  };

  /** Start or resume at the visual timeline position; invoke after a sound gesture. */
  async play(time: number): Promise<boolean> {
    if (this.disposed) return false;
    this.seek(time);
    this.wantsPlayback = true;
    const attempt = ++this.generation;
    if (this.muted) return true;
    const audio = this.ensureAudio();
    if (!audio) return false;
    this.applyTime();
    try {
      await audio.play();
      if (attempt !== this.generation || this.disposed) {
        if (!this.wantsPlayback || this.audio !== audio) audio.pause();
        return false;
      }
      return true;
    } catch {
      // Autoplay rejection, unsupported decoding, or an interrupted play call
      // must never become an unhandled promise rejection.
      return false;
    }
  }

  pause(): void {
    this.wantsPlayback = false;
    this.generation += 1;
    this.audio?.pause();
  }

  seek(time: number): void {
    this.targetTime = Number.isFinite(time)
      ? Math.min(CLOUD_SCORE_DURATION, Math.max(0, time))
      : 0;
    this.applyTime();
  }

  /** Unmuting alone never starts playback; follow with play(time) in the click. */
  setMuted(muted: boolean): void {
    this.muted = muted;
    if (this.audio) this.audio.muted = muted;
    if (muted) this.pause();
  }

  dispose(): void {
    this.pause();
    this.disposed = true;
    if (this.audio) {
      this.audio.removeEventListener('loadedmetadata', this.applyTime);
      this.audio.removeAttribute('src');
      this.audio.load();
      this.audio = null;
    }
  }
}
