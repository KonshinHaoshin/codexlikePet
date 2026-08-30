import type { PetManifest } from "./atlas";

export interface PetSettings {
  scale: number;
  opacity: number;
  speed: number;
  wanderEnabled: boolean;
  clickThrough: boolean;
  lockPosition: boolean;
  quietMode: boolean;
  paused: boolean;
}

export interface PetSettingsEvent {
  petId: string;
  settings: PetSettings;
}

export interface PetInstanceInfo {
  id: string;
  petId: string;
  visible: boolean;
  isMain: boolean;
}

export interface InstalledPetInfo {
  id: string;
  displayName: string;
  description: string;
  spriteVersionNumber: number;
  spritesheetPath: string;
  source: "bundled" | "imported";
  enabled: boolean;
  previewDataUrl: string | null;
  path: string | null;
  settings: PetSettings;
}

export interface RuntimeConfig {
  instanceId: string;
  petId: string;
  source: "bundled" | "imported";
  path: string | null;
  manifest: PetManifest | null;
  spritesheetDataUrl: string | null;
  settings: PetSettings;
}
