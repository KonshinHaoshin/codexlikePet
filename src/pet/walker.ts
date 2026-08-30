import { LogicalPosition } from "@tauri-apps/api/dpi";
import { currentMonitor, getCurrentWindow, primaryMonitor } from "@tauri-apps/api/window";
import { dragState, type DragDirection } from "./window";

const WALK_DELAY_MIN = 30000;
const WALK_DELAY_MAX = 60000;
const WALK_MIN_DISTANCE = 160;
const WALK_SPEED = 95;
const WALK_TICK_MS = 50;

interface WalkBounds {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
}

/** Moves the pet occasionally while leaving long, quiet idle periods. */
export class PetWalker {
  private readonly window = getCurrentWindow();
  private timer: number | null = null;
  private walkToken = 0;
  private walking = false;

  constructor(
    private readonly onChange: (walking: boolean, direction: DragDirection | null) => void,
  ) {}

  start(): void {
    if (this.timer !== null || this.walking) return;
    this.schedule();
  }

  stop(): void {
    this.walkToken += 1;
    if (this.timer !== null) {
      window.clearTimeout(this.timer);
      this.timer = null;
    }
    if (this.walking) {
      this.walking = false;
      this.onChange(false, null);
    }
  }

  private schedule(): void {
    const delay = WALK_DELAY_MIN + Math.random() * (WALK_DELAY_MAX - WALK_DELAY_MIN);
    this.timer = window.setTimeout(() => {
      this.timer = null;
      void this.walk();
    }, delay);
  }

  private async walk(): Promise<void> {
    if (dragState.current) {
      this.schedule();
      return;
    }

    const token = this.walkToken;
    try {
      const monitor = (await currentMonitor()) ?? (await primaryMonitor());
      if (!monitor || token !== this.walkToken || dragState.current) {
        if (token === this.walkToken) this.schedule();
        return;
      }

      const [position, windowSize, scaleFactor] = await Promise.all([
        this.window.outerPosition(),
        this.window.outerSize(),
        this.window.scaleFactor(),
      ]);
      if (token !== this.walkToken || dragState.current) return;

      const workAreaPosition = monitor.workArea.position.toLogical(monitor.scaleFactor);
      const workAreaSize = monitor.workArea.size.toLogical(monitor.scaleFactor);
      const currentPosition = position.toLogical(scaleFactor);
      const currentSize = windowSize.toLogical(scaleFactor);
      const bounds: WalkBounds = {
        minX: workAreaPosition.x,
        maxX: Math.max(workAreaPosition.x, workAreaPosition.x + workAreaSize.width - currentSize.width),
        minY: workAreaPosition.y,
        maxY: Math.max(workAreaPosition.y, workAreaPosition.y + workAreaSize.height - currentSize.height),
      };
      const currentX = Math.min(bounds.maxX, Math.max(bounds.minX, currentPosition.x));
      const currentY = Math.min(bounds.maxY, Math.max(bounds.minY, currentPosition.y));
      const targetX = this.pickTarget(currentX, bounds);
      if (Math.abs(targetX - currentX) < 1) {
        this.schedule();
        return;
      }
      const direction: DragDirection = targetX < currentX ? "left" : "right";
      const duration = Math.max(3500, Math.min(14000, (Math.abs(targetX - currentX) / WALK_SPEED) * 1000));

      this.walking = true;
      this.onChange(true, direction);
      await this.move(token, currentX, currentY, targetX, duration);
    } catch (error) {
      console.warn("autonomous pet walk stopped:", error);
      this.finish(token);
    }
  }

  private pickTarget(currentX: number, bounds: WalkBounds): number {
    const padding = Math.min(24, Math.max(0, (bounds.maxX - bounds.minX) / 2));
    const minX = bounds.minX + padding;
    const maxX = bounds.maxX - padding;
    if (maxX - minX < WALK_MIN_DISTANCE) return currentX;

    let target = minX + Math.random() * (maxX - minX);
    if (Math.abs(target - currentX) < WALK_MIN_DISTANCE) {
      target = currentX < (minX + maxX) / 2 ? maxX : minX;
    }
    return target;
  }

  private async move(
    token: number,
    startX: number,
    y: number,
    targetX: number,
    duration: number,
  ): Promise<void> {
    const startedAt = performance.now();
    while (token === this.walkToken && !dragState.current) {
      const progress = Math.min(1, (performance.now() - startedAt) / duration);
      const x = startX + (targetX - startX) * progress;
      await this.window.setPosition(new LogicalPosition(x, y));
      if (progress >= 1) break;
      await new Promise<void>((resolve) => window.setTimeout(resolve, WALK_TICK_MS));
    }
    this.finish(token);
  }

  private finish(token: number): void {
    if (token !== this.walkToken) return;
    if (this.walking) {
      this.walking = false;
      this.onChange(false, null);
    }
    this.schedule();
  }
}
