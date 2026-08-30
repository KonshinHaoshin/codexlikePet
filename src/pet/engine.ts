import {
  CELL_WIDTH,
  STATE_TIMING,
  type AnimationState,
  type LookDirection,
} from "./atlas";
import { drawLookCell, drawStateFrame } from "./loader";

/**
 * PetEngine drives a single-output canvas from a Codex v2 sprite atlas.
 *
 * - Regular states loop their row with per-frame durations.
 * - A non-null `lookDirection` takes precedence and renders a static
 *   look-direction cell (row 9/10). Setting it back to null resumes the state
 *   animation.
 */
export class PetEngine {
  private readonly source: HTMLCanvasElement;
  private readonly target: CanvasRenderingContext2D;
  private readonly scale: number;

  private state: AnimationState = "idle";
  private stateFrame = 0;
  private stateElapsed = 0;
  private lastTick = performance.now();

  private look: LookDirection | null = null;

  private playing = false;
  private rafId = 0;

  constructor(source: HTMLCanvasElement, target: HTMLCanvasElement, scale = 2) {
    this.source = source;
    this.target = target.getContext("2d")!;
    this.scale = scale;
  }

  setState(state: AnimationState): void {
    if (this.state !== state) {
      this.state = state;
      this.stateFrame = 0;
      this.stateElapsed = 0;
    }
  }

  getState(): AnimationState {
    return this.state;
  }

  setLook(direction: LookDirection | null): void {
    this.look = direction;
  }

  getLook(): LookDirection | null {
    return this.look;
  }

  play(active: boolean): void {
    if (active === this.playing) return;
    this.playing = active;
    if (active) {
      this.lastTick = performance.now();
      this.rafId = requestAnimationFrame(this.tick);
    } else {
      cancelAnimationFrame(this.rafId);
    }
  }

  private readonly tick = (now: number): void => {
    const dt = now - this.lastTick;
    this.lastTick = now;

    if (this.look === null) {
      // Advance the looping state animation using per-frame durations.
      const spec = STATE_TIMING[this.state];
      this.stateElapsed += dt;
      while (this.stateFrame < spec.durations.length && this.stateElapsed >= spec.durations[this.stateFrame]) {
        this.stateElapsed -= spec.durations[this.stateFrame];
        this.stateFrame = (this.stateFrame + 1) % spec.used;
      }
      drawStateFrame(
        this.source,
        this.target,
        this.state,
        this.stateFrame,
        this.scale,
        (this.target.canvas.width - CELL_WIDTH * this.scale) / 2,
        0,
      );
    } else {
      // Static look-direction pose (no frame advancing).
      drawLookCell(
        this.source,
        this.target,
        this.look,
        this.scale,
        (this.target.canvas.width - CELL_WIDTH * this.scale) / 2,
        0,
      );
    }

    if (this.playing) {
      this.rafId = requestAnimationFrame(this.tick);
    }
  };
}