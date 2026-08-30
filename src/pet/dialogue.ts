import type { PetDialogue } from "./config";

export const DEFAULT_DIALOGUE: PetDialogue = {
  version: 1,
  doubleClick: ["嗯？找我吗？", "今天也一起玩吧。"],
  click: ["怎么啦？", "我在这里哦。"],
  rightClick: ["轻一点嘛。"],
  walk: ["我去附近转转。", "散步时间到了！"],
  drag: ["要带我去哪里呀？", "我来啦！"],
  idle: ["这里待着也很舒服。", "要不要陪我说说话？"],
};

function normalizeLines(value: unknown, fallback: string[]): string[] {
  if (!Array.isArray(value)) return fallback;
  return value
    .filter((line): line is string => typeof line === "string")
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 32);
}

function normalizeDialogue(value: unknown): PetDialogue {
  if (!value || typeof value !== "object") return DEFAULT_DIALOGUE;
  const source = value as {
    version?: unknown;
    doubleClick?: unknown;
    click?: unknown;
    rightClick?: unknown;
    walk?: unknown;
    drag?: unknown;
    idle?: unknown;
  };
  const doubleClick = normalizeLines(source.doubleClick, DEFAULT_DIALOGUE.doubleClick);
  return {
    version: source.version === 1 ? 1 : DEFAULT_DIALOGUE.version,
    doubleClick: doubleClick.length ? doubleClick : DEFAULT_DIALOGUE.doubleClick,
    click: normalizeLines(source.click, DEFAULT_DIALOGUE.click),
    rightClick: normalizeLines(source.rightClick, DEFAULT_DIALOGUE.rightClick),
    walk: normalizeLines(source.walk, DEFAULT_DIALOGUE.walk),
    drag: normalizeLines(source.drag, DEFAULT_DIALOGUE.drag),
    idle: normalizeLines(source.idle, DEFAULT_DIALOGUE.idle),
  };
}

export async function loadDialogue(baseUrl: string): Promise<PetDialogue> {
  try {
    const response = await fetch(`${baseUrl}/character.json`);
    if (!response.ok) return DEFAULT_DIALOGUE;
    return normalizeDialogue(await response.json());
  } catch {
    return DEFAULT_DIALOGUE;
  }
}
